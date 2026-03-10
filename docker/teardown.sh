#!/usr/bin/env bash

###############################################################################
# Graphica Infrastructure Teardown Script
#
# Production-ready script to stop and clean up all external dependencies
#
# Usage:
#   ./docker/teardown.sh [OPTIONS]
#
# Options:
#   --keep-data    Keep volumes (preserve data)
#   --full-clean   Remove everything including networks and volumes
#   --force        Skip confirmation prompts
#
# Environment Variables:
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
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-graphica}"
KEEP_DATA=false
FULL_CLEAN=false
FORCE=false

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

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --keep-data)
                KEEP_DATA=true
                shift
                ;;
            --full-clean)
                FULL_CLEAN=true
                shift
                ;;
            --force)
                FORCE=true
                shift
                ;;
            -h|--help)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --keep-data    Keep volumes (preserve data)"
                echo "  --full-clean   Remove everything including networks and volumes"
                echo "  --force        Skip confirmation prompts"
                echo "  -h, --help     Show this help message"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                echo "Use --help for usage information"
                exit 1
                ;;
        esac
    done
}

# Confirm action
confirm_action() {
    if [[ "${FORCE}" == "true" ]]; then
        return 0
    fi

    local message=$1
    echo -e "${YELLOW}${message}${NC}"
    read -p "Continue? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        log_info "Teardown cancelled"
        exit 0
    fi
}

# Show current infrastructure status
show_status() {
    log_info "Current infrastructure status:"
    echo ""

    cd "${PROJECT_ROOT}"

    # Check running containers
    local running_containers
    running_containers=$(docker-compose ps --services --filter "status=running" 2>/dev/null | wc -l)

    if [[ ${running_containers} -eq 0 ]]; then
        log_info "No running containers found"
    else
        docker-compose ps
        echo ""
    fi

    # Check volumes
    local volumes
    volumes=$(docker volume ls --filter "name=${COMPOSE_PROJECT_NAME}" --format "{{.Name}}" 2>/dev/null)
    if [[ -n "${volumes}" ]]; then
        log_info "Existing volumes:"
        echo "${volumes}" | while read -r vol; do
            local size
            size=$(docker volume inspect "${vol}" --format '{{ .Mountpoint }}' 2>/dev/null | xargs du -sh 2>/dev/null | cut -f1 || echo "unknown")
            echo "  - ${vol} (${size})"
        done
        echo ""
    fi
}

# Stop containers
stop_containers() {
    log_info "Stopping containers..."

    cd "${PROJECT_ROOT}"

    # Stop all containers
    docker-compose down --remove-orphans || log_warning "Some containers may have already been stopped"

    log_success "Containers stopped"
}

# Remove volumes
remove_volumes() {
    log_info "Removing volumes..."

    cd "${PROJECT_ROOT}"

    local volumes
    volumes=$(docker volume ls --filter "name=${COMPOSE_PROJECT_NAME}" --format "{{.Name}}" 2>/dev/null)

    if [[ -z "${volumes}" ]]; then
        log_info "No volumes found to remove"
        return
    fi

    echo "${volumes}" | while read -r vol; do
        docker volume rm "${vol}" 2>/dev/null && log_success "Removed volume: ${vol}" || log_warning "Failed to remove volume: ${vol}"
    done
}

# Remove networks
remove_networks() {
    log_info "Removing networks..."

    local network="${COMPOSE_PROJECT_NAME}_graphica-network"

    if docker network inspect "${network}" &>/dev/null; then
        docker network rm "${network}" 2>/dev/null && log_success "Removed network: ${network}" || log_warning "Failed to remove network: ${network}"
    else
        log_info "Network ${network} not found"
    fi
}

# Clean configuration files (optional)
clean_config() {
    if [[ "${FULL_CLEAN}" == "true" ]]; then
        log_info "Cleaning configuration files..."

        local config_dirs=(
            "${PROJECT_ROOT}/docker/prometheus"
            "${PROJECT_ROOT}/docker/grafana"
            "${PROJECT_ROOT}/docker/postgres"
            "${PROJECT_ROOT}/docker/timescaledb"
        )

        for dir in "${config_dirs[@]}"; do
            if [[ -d "${dir}" ]]; then
                rm -rf "${dir}"
                log_success "Removed: ${dir}"
            fi
        done
    fi
}

# Prune unused Docker resources (optional)
prune_docker() {
    if [[ "${FULL_CLEAN}" == "true" ]]; then
        log_info "Pruning unused Docker resources..."

        confirm_action "This will remove all unused Docker images, containers, and networks. Are you sure?"

        docker system prune -af --volumes 2>/dev/null || log_warning "Failed to prune some resources"
        log_success "Docker resources pruned"
    fi
}

# Verify cleanup
verify_cleanup() {
    log_info "Verifying cleanup..."

    cd "${PROJECT_ROOT}"

    # Check containers
    local running_containers
    running_containers=$(docker-compose ps --services --filter "status=running" 2>/dev/null | wc -l)

    if [[ ${running_containers} -gt 0 ]]; then
        log_warning "Some containers are still running"
        docker-compose ps --filter "status=running"
        return 1
    fi

    # Check volumes (if they should be removed)
    if [[ "${KEEP_DATA}" == "false" ]] || [[ "${FULL_CLEAN}" == "true" ]]; then
        local volumes
        volumes=$(docker volume ls --filter "name=${COMPOSE_PROJECT_NAME}" --format "{{.Name}}" 2>/dev/null | wc -l)

        if [[ ${volumes} -gt 0 ]]; then
            log_warning "${volumes} volume(s) still exist"
            return 1
        fi
    fi

    log_success "Cleanup verified"
    return 0
}

# Show cleanup summary
show_summary() {
    echo ""
    log_info "=== Teardown Summary ==="
    echo ""

    if [[ "${KEEP_DATA}" == "true" ]]; then
        log_info "Data volumes were preserved"
        echo "  To remove data later: docker-compose down -v"
    elif [[ "${FULL_CLEAN}" == "true" ]]; then
        log_success "Full cleanup completed (containers, volumes, networks, and configs removed)"
    else
        log_success "Containers and volumes removed"
    fi

    echo ""
    log_info "To restart infrastructure: ./docker/startup.sh [PROFILE]"
    echo ""
}

# Main execution
main() {
    log_info "Graphica Infrastructure Teardown"
    log_info "================================"
    echo ""

    parse_args "$@"

    # Show current status
    show_status

    # Confirm action based on options
    if [[ "${FULL_CLEAN}" == "true" ]]; then
        confirm_action "This will perform a FULL CLEANUP (containers, volumes, networks, configs). All data will be lost!"
    elif [[ "${KEEP_DATA}" == "true" ]]; then
        confirm_action "This will stop containers but keep data volumes. Continue?"
    else
        confirm_action "This will stop containers and remove data volumes. All data will be lost!"
    fi

    # Stop containers
    stop_containers

    # Remove volumes (unless --keep-data)
    if [[ "${KEEP_DATA}" == "false" ]]; then
        remove_volumes
    fi

    # Remove networks (if --full-clean)
    if [[ "${FULL_CLEAN}" == "true" ]]; then
        remove_networks
        clean_config
        prune_docker
    fi

    # Verify cleanup
    verify_cleanup

    # Show summary
    show_summary

    log_success "Teardown completed successfully!"
}

# Run main
main "$@"
