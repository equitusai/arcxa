#!/usr/bin/env bash

###############################################################################
# Graphica Infrastructure Health Check Script
#
# Comprehensive health check for all running services
#
# Usage:
#   ./docker/health-check.sh [OPTIONS]
#
# Options:
#   --json         Output results in JSON format
#   --verbose      Show detailed health information
#   --wait         Wait for all services to be healthy
#
# Exit Codes:
#   0 - All services healthy
#   1 - One or more services unhealthy
#   2 - Error during health check
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
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-graphica}"
JSON_OUTPUT=false
VERBOSE=false
WAIT_MODE=false
WAIT_TIMEOUT=300
WAIT_INTERVAL=5

# Health status
declare -A SERVICE_HEALTH

# Logging functions
log_info() {
    if [[ "${JSON_OUTPUT}" == "false" ]]; then
        echo -e "${BLUE}[INFO]${NC} $1"
    fi
}

log_success() {
    if [[ "${JSON_OUTPUT}" == "false" ]]; then
        echo -e "${GREEN}[✓]${NC} $1"
    fi
}

log_warning() {
    if [[ "${JSON_OUTPUT}" == "false" ]]; then
        echo -e "${YELLOW}[⚠]${NC} $1"
    fi
}

log_error() {
    if [[ "${JSON_OUTPUT}" == "false" ]]; then
        echo -e "${RED}[✗]${NC} $1"
    fi
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --json)
                JSON_OUTPUT=true
                shift
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            --wait)
                WAIT_MODE=true
                shift
                ;;
            -h|--help)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --json         Output results in JSON format"
                echo "  --verbose      Show detailed health information"
                echo "  --wait         Wait for all services to be healthy"
                echo "  -h, --help     Show this help message"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 2
                ;;
        esac
    done
}

# Check if Docker Compose is available
check_docker_compose() {
    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        log_error "Docker Compose is not available"
        exit 2
    fi
}

# Check service health via Docker
check_docker_health() {
    local service=$1
    local container_name="${COMPOSE_PROJECT_NAME}-${service}"

    # Check if container exists
    if ! docker ps -a --format '{{.Names}}' | grep -q "^${container_name}$"; then
        SERVICE_HEALTH["${service}"]="not_running"
        return 1
    fi

    # Check container status
    local status
    status=$(docker inspect --format='{{.State.Status}}' "${container_name}" 2>/dev/null || echo "unknown")

    if [[ "${status}" != "running" ]]; then
        SERVICE_HEALTH["${service}"]="stopped"
        return 1
    fi

    # Check health status
    local health
    health=$(docker inspect --format='{{.State.Health.Status}}' "${container_name}" 2>/dev/null || echo "none")

    if [[ "${health}" == "none" ]]; then
        # No health check defined, assume healthy if running
        SERVICE_HEALTH["${service}"]="healthy"
        return 0
    elif [[ "${health}" == "healthy" ]]; then
        SERVICE_HEALTH["${service}"]="healthy"
        return 0
    else
        SERVICE_HEALTH["${service}"]="${health}"
        return 1
    fi
}

# Check Kafka connectivity
check_kafka() {
    log_info "Checking Kafka..."

    if ! check_docker_health "kafka"; then
        log_error "Kafka: ${SERVICE_HEALTH["kafka"]}"
        return 1
    fi

    # Additional Kafka-specific checks
    if [[ "${VERBOSE}" == "true" ]]; then
        local topics
        topics=$(docker exec graphica-kafka kafka-topics --list --bootstrap-server localhost:9092 2>/dev/null | wc -l || echo "0")
        log_info "Kafka topics count: ${topics}"
    fi

    log_success "Kafka: healthy"
    return 0
}

# Check Zookeeper connectivity
check_zookeeper() {
    log_info "Checking Zookeeper..."

    if ! check_docker_health "zookeeper"; then
        log_error "Zookeeper: ${SERVICE_HEALTH["zookeeper"]}"
        return 1
    fi

    log_success "Zookeeper: healthy"
    return 0
}

# Check Schema Registry
check_schema_registry() {
    log_info "Checking Schema Registry..."

    if ! check_docker_health "schema-registry"; then
        log_error "Schema Registry: ${SERVICE_HEALTH["schema-registry"]}"
        return 1
    fi

    # Additional check: API endpoint
    if [[ "${VERBOSE}" == "true" ]]; then
        if curl -s http://localhost:8081/ > /dev/null 2>&1; then
            log_info "Schema Registry API: accessible"
        else
            log_warning "Schema Registry API: not accessible"
        fi
    fi

    log_success "Schema Registry: healthy"
    return 0
}

# Check PostgreSQL
check_postgres() {
    if ! docker ps --format '{{.Names}}' | grep -q "graphica-postgres"; then
        return 0  # Not running, skip
    fi

    log_info "Checking PostgreSQL..."

    if ! check_docker_health "postgres"; then
        log_error "PostgreSQL: ${SERVICE_HEALTH["postgres"]}"
        return 1
    fi

    # Additional check: Database connectivity
    if [[ "${VERBOSE}" == "true" ]]; then
        if docker exec graphica-postgres pg_isready -U graphica > /dev/null 2>&1; then
            log_info "PostgreSQL: accepting connections"
        else
            log_warning "PostgreSQL: not accepting connections"
        fi
    fi

    log_success "PostgreSQL: healthy"
    return 0
}

# Check TimescaleDB
check_timescaledb() {
    if ! docker ps --format '{{.Names}}' | grep -q "graphica-timescaledb"; then
        return 0  # Not running, skip
    fi

    log_info "Checking TimescaleDB..."

    if ! check_docker_health "timescaledb"; then
        log_error "TimescaleDB: ${SERVICE_HEALTH["timescaledb"]}"
        return 1
    fi

    log_success "TimescaleDB: healthy"
    return 0
}

# Check Redis
check_redis() {
    if ! docker ps --format '{{.Names}}' | grep -q "graphica-redis"; then
        return 0  # Not running, skip
    fi

    log_info "Checking Redis..."

    if ! check_docker_health "redis"; then
        log_error "Redis: ${SERVICE_HEALTH["redis"]}"
        return 1
    fi

    # Additional check: PING command
    if [[ "${VERBOSE}" == "true" ]]; then
        if docker exec graphica-redis redis-cli PING 2>/dev/null | grep -q PONG; then
            log_info "Redis: responding to PING"
        else
            log_warning "Redis: not responding to PING"
        fi
    fi

    log_success "Redis: healthy"
    return 0
}

# Check Prometheus
check_prometheus() {
    if ! docker ps --format '{{.Names}}' | grep -q "graphica-prometheus"; then
        return 0  # Not running, skip
    fi

    log_info "Checking Prometheus..."

    if ! check_docker_health "prometheus"; then
        log_error "Prometheus: ${SERVICE_HEALTH["prometheus"]}"
        return 1
    fi

    log_success "Prometheus: healthy"
    return 0
}

# Check Grafana
check_grafana() {
    if ! docker ps --format '{{.Names}}' | grep -q "graphica-grafana"; then
        return 0  # Not running, skip
    fi

    log_info "Checking Grafana..."

    if ! check_docker_health "grafana"; then
        log_error "Grafana: ${SERVICE_HEALTH["grafana"]}"
        return 1
    fi

    log_success "Grafana: healthy"
    return 0
}

# Run all health checks
run_health_checks() {
    local all_healthy=true

    check_zookeeper || all_healthy=false
    check_kafka || all_healthy=false
    check_schema_registry || all_healthy=false
    check_postgres || all_healthy=false
    check_timescaledb || all_healthy=false
    check_redis || all_healthy=false
    check_prometheus || all_healthy=false
    check_grafana || all_healthy=false

    if [[ "${all_healthy}" == "true" ]]; then
        return 0
    else
        return 1
    fi
}

# Wait for services to be healthy
wait_for_health() {
    local max_attempts=$((WAIT_TIMEOUT / WAIT_INTERVAL))
    local attempt=0

    log_info "Waiting for services to be healthy (timeout: ${WAIT_TIMEOUT}s)..."

    while [[ ${attempt} -lt ${max_attempts} ]]; do
        if run_health_checks; then
            log_success "All services are healthy"
            return 0
        fi

        attempt=$((attempt + 1))
        if [[ "${JSON_OUTPUT}" == "false" ]]; then
            echo -n "."
        fi
        sleep ${WAIT_INTERVAL}
    done

    if [[ "${JSON_OUTPUT}" == "false" ]]; then
        echo ""
    fi
    log_error "Timeout waiting for services to be healthy"
    return 1
}

# Output JSON results
output_json() {
    local status=$1
    local timestamp
    timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    echo "{"
    echo "  \"timestamp\": \"${timestamp}\","
    echo "  \"status\": \"${status}\","
    echo "  \"services\": {"

    local first=true
    for service in "${!SERVICE_HEALTH[@]}"; do
        if [[ "${first}" == "true" ]]; then
            first=false
        else
            echo ","
        fi
        echo -n "    \"${service}\": \"${SERVICE_HEALTH[${service}]}\""
    done

    echo ""
    echo "  }"
    echo "}"
}

# Main execution
main() {
    parse_args "$@"
    check_docker_compose

    cd "${PROJECT_ROOT}"

    if [[ "${WAIT_MODE}" == "true" ]]; then
        if wait_for_health; then
            if [[ "${JSON_OUTPUT}" == "true" ]]; then
                output_json "healthy"
            fi
            exit 0
        else
            if [[ "${JSON_OUTPUT}" == "true" ]]; then
                output_json "unhealthy"
            fi
            exit 1
        fi
    else
        if [[ "${JSON_OUTPUT}" == "false" ]]; then
            log_info "Graphica Infrastructure Health Check"
            log_info "===================================="
            echo ""
        fi

        if run_health_checks; then
            if [[ "${JSON_OUTPUT}" == "true" ]]; then
                output_json "healthy"
            else
                echo ""
                log_success "All services are healthy"
            fi
            exit 0
        else
            if [[ "${JSON_OUTPUT}" == "true" ]]; then
                output_json "unhealthy"
            else
                echo ""
                log_error "Some services are unhealthy"
            fi
            exit 1
        fi
    fi
}

# Run main
main "$@"
