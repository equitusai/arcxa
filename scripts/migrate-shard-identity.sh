#!/bin/bash

# Graphica Shard Identity Migration Script
# Migrates existing shards from manual ID configuration to auto-discovery
#
# Usage: ./migrate-shard-identity.sh --data-path /data/shard --shard-id 0
#        ./migrate-shard-identity.sh --data-path /data/shard --shard-id 0 --coordinator coordinator:9090

set -euo pipefail

# Default values
DATA_PATH=""
SHARD_ID=""
COORDINATOR_URL="localhost:9090"
MACHINE_ID=""
FORCE=false
BACKUP=true

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Print usage
usage() {
    cat << EOF
Graphica Shard Identity Migration Script

This script creates an identity file for existing shards to enable auto-discovery.

Usage: $0 [OPTIONS]

Required Options:
    --data-path PATH        Path to shard data directory
    --shard-id ID          Current shard ID

Optional Options:
    --coordinator URL      Coordinator URL (default: localhost:9090)
    --machine-id UUID      Machine ID (auto-generated if not provided)
    --force               Overwrite existing identity file
    --no-backup          Skip backup of existing identity
    -h, --help           Show this help message

Examples:
    # Migrate shard 0
    $0 --data-path /data/shard-0 --shard-id 0

    # Migrate with specific coordinator
    $0 --data-path /data/shard-0 --shard-id 0 --coordinator coordinator.prod:9090

    # Force overwrite existing identity
    $0 --data-path /data/shard-0 --shard-id 0 --force

EOF
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --data-path)
            DATA_PATH="$2"
            shift 2
            ;;
        --shard-id)
            SHARD_ID="$2"
            shift 2
            ;;
        --coordinator)
            COORDINATOR_URL="$2"
            shift 2
            ;;
        --machine-id)
            MACHINE_ID="$2"
            shift 2
            ;;
        --force)
            FORCE=true
            shift
            ;;
        --no-backup)
            BACKUP=false
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            usage
            ;;
    esac
done

# Validate required arguments
if [[ -z "$DATA_PATH" ]]; then
    echo -e "${RED}Error: --data-path is required${NC}"
    usage
fi

if [[ -z "$SHARD_ID" ]]; then
    echo -e "${RED}Error: --shard-id is required${NC}"
    usage
fi

# Validate data path exists
if [[ ! -d "$DATA_PATH" ]]; then
    echo -e "${RED}Error: Data path does not exist: $DATA_PATH${NC}"
    exit 1
fi

# Generate machine ID if not provided
if [[ -z "$MACHINE_ID" ]]; then
    if command -v uuidgen &> /dev/null; then
        MACHINE_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
    elif command -v uuid &> /dev/null; then
        MACHINE_ID=$(uuid)
    else
        echo -e "${RED}Error: Cannot generate UUID. Please install uuidgen or provide --machine-id${NC}"
        exit 1
    fi
    echo -e "${YELLOW}Generated machine ID: $MACHINE_ID${NC}"
fi

# Validate machine ID format
if ! echo "$MACHINE_ID" | grep -qE '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'; then
    echo -e "${RED}Error: Invalid machine ID format. Expected UUID format.${NC}"
    exit 1
fi

# Create .graphica directory
IDENTITY_DIR="$DATA_PATH/.graphica"
IDENTITY_FILE="$IDENTITY_DIR/shard_identity.json"

if [[ ! -d "$IDENTITY_DIR" ]]; then
    echo "Creating directory: $IDENTITY_DIR"
    mkdir -p "$IDENTITY_DIR"
fi

# Check if identity file already exists
if [[ -f "$IDENTITY_FILE" ]] && [[ "$FORCE" != true ]]; then
    echo -e "${YELLOW}Warning: Identity file already exists: $IDENTITY_FILE${NC}"
    echo "Use --force to overwrite"
    exit 1
fi

# Backup existing identity if requested
if [[ -f "$IDENTITY_FILE" ]] && [[ "$BACKUP" == true ]]; then
    BACKUP_FILE="${IDENTITY_FILE}.backup.$(date +%Y%m%d_%H%M%S)"
    echo "Backing up existing identity to: $BACKUP_FILE"
    cp "$IDENTITY_FILE" "$BACKUP_FILE"
fi

# Calculate hash range (simplified - in production this would query coordinator)
# For migration, we'll set these as null to be filled during registration
HASH_START="null"
HASH_END="null"

# Get current timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Get hostname and IP
HOSTNAME=$(hostname -f 2>/dev/null || hostname)
IP_ADDRESS=$(ip -4 addr show | grep -oP '(?<=inet\s)\d+(\.\d+){3}' | grep -v '127.0.0.1' | head -1 || echo "")

# Create identity JSON
cat > "$IDENTITY_FILE" << EOF
{
  "shard_id": $SHARD_ID,
  "machine_id": "$MACHINE_ID",
  "first_registered": "$TIMESTAMP",
  "last_started": "$TIMESTAMP",
  "coordinator_url": "$COORDINATOR_URL",
  "hash_range": {
    "start": $HASH_START,
    "end": $HASH_END
  },
  "version": 1,
  "metadata": {
    "hostname": "$HOSTNAME",
    "ip_address": "$IP_ADDRESS",
    "datacenter": null,
    "rack": null,
    "labels": {
      "migrated": "true",
      "migration_date": "$TIMESTAMP",
      "original_shard_id": "$SHARD_ID"
    }
  }
}
EOF

# Verify JSON is valid
if command -v jq &> /dev/null; then
    if ! jq empty "$IDENTITY_FILE" 2>/dev/null; then
        echo -e "${RED}Error: Generated invalid JSON${NC}"
        exit 1
    fi
    # Pretty print the file
    jq . "$IDENTITY_FILE" > "$IDENTITY_FILE.tmp" && mv "$IDENTITY_FILE.tmp" "$IDENTITY_FILE"
fi

# Set appropriate permissions
chmod 600 "$IDENTITY_FILE"

echo -e "${GREEN}✓ Successfully created identity file: $IDENTITY_FILE${NC}"
echo ""
echo "Identity Summary:"
echo "  Shard ID:      $SHARD_ID"
echo "  Machine ID:    $MACHINE_ID"
echo "  Coordinator:   $COORDINATOR_URL"
echo "  Data Path:     $DATA_PATH"
echo ""
echo "Next Steps:"
echo "1. Stop the shard process if running"
echo "2. Start the shard without --shard-id flag:"
echo "   ./graphica-shard --data-path $DATA_PATH --coordinator-url $COORDINATOR_URL"
echo "3. The shard will reconnect using the identity file"
echo ""
echo -e "${YELLOW}Note: The hash range will be updated automatically during reconnection.${NC}"

# Optionally validate connectivity to coordinator
if command -v nc &> /dev/null; then
    COORD_HOST=$(echo "$COORDINATOR_URL" | cut -d: -f1)
    COORD_PORT=$(echo "$COORDINATOR_URL" | cut -d: -f2)

    echo ""
    echo -n "Testing coordinator connectivity... "
    if nc -z -w2 "$COORD_HOST" "$COORD_PORT" 2>/dev/null; then
        echo -e "${GREEN}✓ Coordinator is reachable${NC}"
    else
        echo -e "${YELLOW}⚠ Could not reach coordinator at $COORDINATOR_URL${NC}"
        echo "  Please ensure the coordinator is running before starting the shard."
    fi
fi

exit 0