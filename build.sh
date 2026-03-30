#!/bin/bash
# Build all ARCXA components

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

VERSION="1.0.0"
MIN_RUST_VERSION="1.91.1"
RUST_TOOLCHAIN="stable"

version_ge() {
    [ "$(printf '%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]
}

check_rust_version() {
    if ! command -v rustc >/dev/null 2>&1; then
        echo -e "${RED}rustc is not installed.${NC}"
        exit 1
    fi

    local current_version
    current_version="$(rustc +${RUST_TOOLCHAIN} --version | awk '{print $2}')"

    if ! version_ge "$current_version" "$MIN_RUST_VERSION"; then
        echo -e "${RED}Rust ${MIN_RUST_VERSION}+ is required (found ${current_version}).${NC}"
        echo "Install and select the required toolchain:"
        echo "  rustup toolchain install ${MIN_RUST_VERSION}"
        echo "  rustup override set ${MIN_RUST_VERSION}"
        exit 1
    fi
}

run_cargo() {
    local cargo_args=("$@")
    if [ "${cargo_args[0]}" = "cargo" ]; then
        cargo_args=("${cargo_args[@]:1}")
    fi

    env -u CC \
        -u AR \
        -u RANLIB \
        -u NM \
        -u LD \
        -u CFLAGS \
        -u CPPFLAGS \
        -u LDFLAGS \
        -u C_INCLUDE_PATH \
        -u CPLUS_INCLUDE_PATH \
        -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig \
        RUSTFLAGS="" \
        cargo +${RUST_TOOLCHAIN} "${cargo_args[@]}"
}

# Show help
show_help() {
    echo "ARCXA Build Script v${VERSION}"
    echo ""
    echo "Usage: ./build.sh [MODE] [OPTIONS]"
    echo ""
    echo "BUILD MODES:"
    echo "  release         Build in release mode (optimized, default)"
    echo "  debug           Build in debug mode (faster builds, slower runtime)"
    echo ""
    echo "OPTIONS:"
    echo "  --help          Show this help message"
    echo "  --status        Show current build configuration and binary status"
    echo ""
    echo "ENVIRONMENT VARIABLES:"
    echo "  ENABLE_AUDIT=true|false        Enable cryptographic audit (default: true)"
    echo "  ENABLE_HA=true|false           Enable HA Raft consensus (default: false)"
    echo "  ENABLE_ODBC=true|false         Enable ODBC-backed connectors for local builds (default: false)"
    echo "  ENABLE_DB2=true|false          Backward-compatible alias for ENABLE_ODBC"
    echo ""
    echo "ALWAYS ENABLED FEATURES:"
    echo "  workflow-storage               Streaming deduplication with RocksDB (always on)"
    echo ""
    echo "EXAMPLES:"
    echo "  ./build.sh                     # Build release with local/dev default features"
    echo "  ./build.sh debug               # Build debug mode"
    echo "  ./build.sh --status            # Show build status without building"
    echo "  ENABLE_HA=true ./build.sh      # Build with HA support"
    echo "  ENABLE_ODBC=true ./build.sh    # Build local binary with Oracle/DB2/SAP HANA connector code"
    echo "  ENABLE_AUDIT=false ./build.sh debug  # Build debug without audit"
    echo ""
    echo "BINARIES:"
    echo "  Coordinator:   ./target/{release|debug}/arcxa-coordinator"
    echo "  Model Service: ./target/{release|debug}/arcxa-model-service"
    echo "  Shard:         ./arcxa-shard/target/{release|debug}/arcxa-shard"
    echo ""
}

# Show build status
show_status() {
    echo "========================================="
    echo "ARCXA Build Status v${VERSION}"
    echo "========================================="
    echo ""

    # Detect current configuration
    ENABLE_AUDIT=${ENABLE_AUDIT:-true}
    ENABLE_HA=${ENABLE_HA:-false}
    if [ -z "${ENABLE_ODBC+x}" ] && [ -n "${ENABLE_DB2+x}" ]; then
        ENABLE_ODBC="${ENABLE_DB2}"
    fi
    ENABLE_ODBC=${ENABLE_ODBC:-false}

    echo "Current Configuration:"
    echo "  Cryptographic Audit: $([ "$ENABLE_AUDIT" = "true" ] && echo -e "${GREEN}ENABLED${NC}" || echo -e "${RED}DISABLED${NC}")"
    echo "  HA Raft Consensus:   $([ "$ENABLE_HA" = "true" ] && echo -e "${GREEN}ENABLED${NC}" || echo -e "${RED}DISABLED${NC}")"
    echo "  ODBC Connectors:     $([ "$ENABLE_ODBC" = "true" ] && echo -e "${GREEN}ENABLED${NC}" || echo -e "${RED}DISABLED${NC}")"
    echo ""

    echo "Release Binaries:"
    for binary in "./target/release/arcxa-coordinator" "./target/release/arcxa-model-service" "./arcxa-shard/target/release/arcxa-shard"; do
        if [ -f "$binary" ]; then
            SIZE=$(ls -lh "$binary" | awk '{print $5}')
            DATE=$(ls -lh "$binary" | awk '{print $6, $7, $8}')
            echo "  ✓ $(basename $binary)"
            echo "    Path:     $binary"
            echo "    Size:     $SIZE"
            echo "    Modified: $DATE"
        else
            echo "  ✗ $(basename $binary) - NOT BUILT"
        fi
        echo ""
    done

    echo "Debug Binaries:"
    for binary in "./target/debug/arcxa-coordinator" "./target/debug/arcxa-model-service" "./arcxa-shard/target/debug/arcxa-shard"; do
        if [ -f "$binary" ]; then
            SIZE=$(ls -lh "$binary" | awk '{print $5}')
            DATE=$(ls -lh "$binary" | awk '{print $6, $7, $8}')
            echo "  ✓ $(basename $binary)"
            echo "    Path:     $binary"
            echo "    Size:     $SIZE"
            echo "    Modified: $DATE"
        else
            echo "  ✗ $(basename $binary) - NOT BUILT"
        fi
        echo ""
    done

    echo "Running Processes:"
    if pgrep -f arcxa-coordinator > /dev/null; then
        COORD_PID=$(pgrep -f arcxa-coordinator)
        COORD_EXE=$(readlink -f /proc/$COORD_PID/exe 2>/dev/null || echo "unknown")
        echo "  ⚠ Coordinator:   RUNNING (PID $COORD_PID)"
        echo "                   Exe: $COORD_EXE"
    else
        echo "  ○ Coordinator:   NOT RUNNING"
    fi

    if pgrep -f arcxa-model-service > /dev/null; then
        MODEL_PID=$(pgrep -f arcxa-model-service)
        MODEL_EXE=$(readlink -f /proc/$MODEL_PID/exe 2>/dev/null || echo "unknown")
        echo "  ⚠ Model Service: RUNNING (PID $MODEL_PID)"
        echo "                   Exe: $MODEL_EXE"
    else
        echo "  ○ Model Service: NOT RUNNING"
    fi

    if pgrep -f arcxa-shard > /dev/null; then
        SHARD_PID=$(pgrep -f arcxa-shard)
        SHARD_EXE=$(readlink -f /proc/$SHARD_PID/exe 2>/dev/null || echo "unknown")
        echo "  ⚠ Shard:         RUNNING (PID $SHARD_PID)"
        echo "                   Exe: $SHARD_EXE"
    else
        echo "  ○ Shard:         NOT RUNNING"
    fi
    echo ""

    # Check workspace version
    if [ -f "Cargo.toml" ]; then
        WORKSPACE_VERSION=$(grep -A 2 '^\[workspace.package\]' Cargo.toml | grep '^version' | cut -d'"' -f2)
        echo "Workspace Version: $WORKSPACE_VERSION"
    fi
    echo ""
    echo "========================================="
}

# Parse arguments
if [ "$1" = "--help" ]; then
    show_help
    exit 0
fi

if [ "$1" = "--status" ]; then
    show_status
    exit 0
fi

check_rust_version

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Building ARCXA Components v${VERSION}${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Detect build mode (debug or release)
BUILD_MODE=${1:-release}

if [ "$BUILD_MODE" = "debug" ]; then
    BUILD_FLAG=""
    BUILD_DIR="debug"
    echo -e "${YELLOW}Building in DEBUG mode${NC}"
else
    BUILD_FLAG="--release"
    BUILD_DIR="release"
    echo -e "${YELLOW}Building in RELEASE mode${NC}"
fi

# Cryptographic audit support (enabled by default)
# Set ENABLE_AUDIT=false to disable
ENABLE_AUDIT=${ENABLE_AUDIT:-true}

# HA Raft consensus support (disabled by default for compatibility)
# Set ENABLE_HA=true to enable
ENABLE_HA=${ENABLE_HA:-false}

# ODBC-backed connector support (disabled by default for driverless local builds)
# Set ENABLE_ODBC=true to build Oracle/DB2/SAP HANA support.
# ENABLE_DB2 remains as a backward-compatible alias.
if [ -z "${ENABLE_ODBC+x}" ] && [ -n "${ENABLE_DB2+x}" ]; then
    ENABLE_ODBC="${ENABLE_DB2}"
fi
ENABLE_ODBC=${ENABLE_ODBC:-false}

# Build feature list for coordinator
COORDINATOR_FEATURES=""

# workflow-storage is now a default feature in coordinator
echo -e "${YELLOW}Workflow Storage (Streaming Dedup): ${GREEN}ENABLED (default)${NC}"

if [ "$ENABLE_AUDIT" = "true" ]; then
    COORDINATOR_FEATURES="cryptographic-audit"
    echo -e "${YELLOW}Cryptographic audit: ${GREEN}ENABLED${NC}"
else
    echo -e "${YELLOW}Cryptographic audit: ${RED}DISABLED${NC}"
fi

if [ "$ENABLE_HA" = "true" ]; then
    if [ -n "$COORDINATOR_FEATURES" ]; then
        COORDINATOR_FEATURES="${COORDINATOR_FEATURES},raft-consensus"
    else
        COORDINATOR_FEATURES="raft-consensus"
    fi
    echo -e "${YELLOW}HA Raft Consensus: ${GREEN}ENABLED${NC}"
else
    echo -e "${YELLOW}HA Raft Consensus: ${RED}DISABLED${NC}"
fi

if [ "$ENABLE_ODBC" = "true" ]; then
    if [ -n "$COORDINATOR_FEATURES" ]; then
        COORDINATOR_FEATURES="${COORDINATOR_FEATURES},odbc"
    else
        COORDINATOR_FEATURES="odbc"
    fi
    echo -e "${YELLOW}ODBC-backed connectors: ${GREEN}ENABLED${NC}"
else
    echo -e "${YELLOW}ODBC-backed connectors: ${RED}DISABLED${NC}"
fi

if [ -n "$COORDINATOR_FEATURES" ]; then
    FEATURE_FLAG="--features ${COORDINATOR_FEATURES}"
else
    FEATURE_FLAG=""
fi
echo ""

# Note: This is a Cargo workspace - binaries go to ./target/${BUILD_DIR}/

# Build workspace components (coordinator, model-service, core)
echo -e "${BLUE}[1/2] Building workspace components (coordinator, model-service, core)...${NC}"
echo -e "${YELLOW}Note: Clearing conda environment variables to avoid OpenSSL conflicts${NC}"

run_cargo cargo build $BUILD_FLAG $FEATURE_FLAG

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Workspace components built successfully${NC}"
    echo -e "   Coordinator:   ./target/${BUILD_DIR}/arcxa-coordinator"
    if [ -n "$COORDINATOR_FEATURES" ]; then
        echo -e "   Features:      ${GREEN}${COORDINATOR_FEATURES}${NC}"
    fi
    echo -e "   Model Service: ./target/${BUILD_DIR}/arcxa-model-service"
    echo -e "   ${YELLOW}(Model service uses load-dynamic ort - set ORT_DYLIB_PATH at runtime)${NC}"
else
    echo -e "${RED}✗ Workspace build failed${NC}"
    exit 1
fi
echo ""

# Build arcxa-shard separately (excluded from workspace due to RocksDB conflict)
echo -e "${BLUE}[2/2] Building arcxa-shard (separate build)...${NC}"
echo -e "${YELLOW}Note: Clearing conda environment variables to avoid OpenSSL conflicts${NC}"
cd arcxa-shard
run_cargo cargo build $BUILD_FLAG
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ arcxa-shard built successfully${NC}"
    echo -e "   Binary: ./arcxa-shard/target/${BUILD_DIR}/arcxa-shard"
else
    echo -e "${RED}✗ arcxa-shard build failed${NC}"
    exit 1
fi
echo ""

# Return to root
cd ..

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Build Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}Binaries (Workspace):${NC}"
echo -e "  Coordinator:   ${YELLOW}./target/${BUILD_DIR}/arcxa-coordinator${NC}"
if [ -n "$COORDINATOR_FEATURES" ]; then
    echo -e "                 ${GREEN}Features: ${COORDINATOR_FEATURES}${NC}"
fi
echo -e "  Model Service: ${YELLOW}./target/${BUILD_DIR}/arcxa-model-service${NC}"
echo ""
echo -e "${BLUE}Binaries (Separate):${NC}"
echo -e "  Shard:         ${YELLOW}./arcxa-shard/target/${BUILD_DIR}/arcxa-shard${NC}"
echo ""
echo -e "${BLUE}Next steps:${NC}"
echo -e "  1. Start infrastructure: ${YELLOW}docker compose up -d zookeeper kafka schema-registry${NC}"
echo -e "  2. Start observability:  ${YELLOW}docker compose up -d prometheus grafana${NC}"
if [ "$ENABLE_HA" = "true" ]; then
    echo -e "  3. Run HA cluster:       ${YELLOW}./run-local-ha.sh${NC}"
else
    echo -e "  3. Run locally:          ${YELLOW}./run-local.sh${NC}"
fi
echo -e "  4. Or run in Docker:     ${YELLOW}docker compose up -d${NC}"
echo ""
if [ "$ENABLE_AUDIT" = "false" ]; then
    echo -e "${YELLOW}Note: Cryptographic audit is DISABLED${NC}"
    echo -e "  To enable: ${YELLOW}ENABLE_AUDIT=true ./build.sh${NC}"
    echo ""
fi
if [ "$ENABLE_HA" = "false" ]; then
    echo -e "${YELLOW}Note: HA Raft consensus is DISABLED${NC}"
    echo -e "  To enable: ${YELLOW}ENABLE_HA=true ./build.sh${NC}"
    echo ""
fi
if [ "$ENABLE_ODBC" = "false" ]; then
    echo -e "${YELLOW}Note: ODBC-backed connectors are DISABLED${NC}"
    echo -e "  To enable Oracle/DB2/SAP HANA support: ${YELLOW}ENABLE_ODBC=true ./build.sh${NC}"
    echo ""
fi
echo -e "${BLUE}Build options:${NC}"
echo -e "  Debug build:       ${YELLOW}./build.sh debug${NC}"
echo -e "  Without audit:     ${YELLOW}ENABLE_AUDIT=false ./build.sh${NC}"
echo -e "  With ODBC:         ${YELLOW}ENABLE_ODBC=true ./build.sh${NC}"
echo -e "  With HA support:   ${YELLOW}ENABLE_HA=true ./build.sh${NC}"
echo -e "  Full enterprise:   ${YELLOW}ENABLE_HA=true ENABLE_AUDIT=true ENABLE_ODBC=true ./build.sh${NC}"
echo ""
echo -e "${BLUE}ODBC Build Profile:${NC}"
echo -e "  Driverless local builds are the default"
echo -e "  Enable Oracle/DB2/SAP HANA support with: ${YELLOW}ENABLE_ODBC=true ./build.sh${NC}"
echo -e "  ODBC build prerequisites: ${YELLOW}sudo apt-get install unixodbc-dev${NC}"
echo ""
echo -e "${BLUE}Observability:${NC}"
echo -e "  Prometheus: ${YELLOW}http://localhost:9090${NC}"
echo -e "  Grafana:    ${YELLOW}http://localhost:3000${NC} (admin/graphica-admin)"
echo ""
