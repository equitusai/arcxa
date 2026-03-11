#!/bin/bash
# Test ARCXA components (Conda-safe)

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

VERSION="1.0.0"

show_help() {
    echo "ARCXA Test Script v${VERSION}"
    echo ""
    echo "Usage: ./test.sh [TARGET] [OPTIONS] [-- <cargo test args>]"
    echo ""
    echo "TARGETS:"
    echo "  workspace       Test coordinator, model-service, core (default)"
    echo "  coordinator     Test arcxa-coordinator"
    echo "  model-service   Test arcxa-model-service"
    echo "  core            Test arcxa-core"
    echo "  shard           Test arcxa-shard (separate crate)"
    echo "  all             Test workspace + shard"
    echo ""
    echo "OPTIONS:"
    echo "  --help          Show this help message"
    echo "  --status        Show current test configuration"
    echo "  --release       Run tests in release mode"
    echo "  --debug         Run tests in debug mode (default)"
    echo ""
    echo "ENVIRONMENT VARIABLES:"
    echo "  ENABLE_AUDIT=true|false        Enable cryptographic audit features (default: true)"
    echo "  ENABLE_HA=true|false           Enable HA Raft consensus features (default: false)"
    echo "  ENABLE_DB2=true|false          Enable DB2/ODBC features (default: true)"
    echo ""
    echo "EXAMPLES:"
    echo "  ./test.sh                      # Test workspace (debug)"
    echo "  ./test.sh core                 # Test arcxa-core"
    echo "  ./test.sh coordinator -- --nocapture"
    echo "  ./test.sh --release            # Release-mode tests"
    echo "  ENABLE_DB2=false ./test.sh coordinator"
    echo ""
}

show_status() {
    echo "========================================="
    echo "ARCXA Test Status v${VERSION}"
    echo "========================================="
    echo ""

    ENABLE_AUDIT=${ENABLE_AUDIT:-true}
    ENABLE_HA=${ENABLE_HA:-false}
    ENABLE_DB2=${ENABLE_DB2:-true}

    echo "Current Configuration:"
    echo "  Cryptographic Audit: $([ "$ENABLE_AUDIT" = "true" ] && echo -e "${GREEN}ENABLED${NC}" || echo -e "${RED}DISABLED${NC}")"
    echo "  HA Raft Consensus:   $([ "$ENABLE_HA" = "true" ] && echo -e "${GREEN}ENABLED${NC}" || echo -e "${RED}DISABLED${NC}")"
    echo "  DB2/ODBC Support:    $([ "$ENABLE_DB2" = "true" ] && echo -e "${GREEN}ENABLED${NC}" || echo -e "${RED}DISABLED${NC}")"
    echo ""
    echo "Conda-safe environment variables will be cleared before running tests."
    echo ""
    echo "========================================="
}

run_cargo() {
    env -u CC \
        -u CFLAGS \
        -u CPPFLAGS \
        -u LDFLAGS \
        -u C_INCLUDE_PATH \
        -u CPLUS_INCLUDE_PATH \
        -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig \
        RUSTFLAGS="" \
        "$@"
}

# Defaults
TARGET="workspace"
BUILD_MODE="debug"
CARGO_ARGS=()
TEST_ARGS=()

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)
            show_help
            exit 0
            ;;
        --status)
            show_status
            exit 0
            ;;
        --release)
            BUILD_MODE="release"
            shift
            ;;
        --debug)
            BUILD_MODE="debug"
            shift
            ;;
        workspace|coordinator|model-service|core|shard|all)
            TARGET="$1"
            shift
            ;;
        --)
            shift
            TEST_ARGS+=("$@")
            break
            ;;
        *)
            CARGO_ARGS+=("$1")
            shift
            ;;
    esac
done

# If user passed test filter after --, move non-flag args back before --
while [ ${#TEST_ARGS[@]} -gt 0 ]; do
    case "${TEST_ARGS[0]}" in
        -*)
            break
            ;;
        *)
            CARGO_ARGS+=("${TEST_ARGS[0]}")
            TEST_ARGS=("${TEST_ARGS[@]:1}")
            ;;
    esac
done

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Running ARCXA Tests v${VERSION}${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

if [ "$BUILD_MODE" = "release" ]; then
    MODE_FLAG="--release"
    echo -e "${YELLOW}Running tests in RELEASE mode${NC}"
else
    MODE_FLAG=""
    echo -e "${YELLOW}Running tests in DEBUG mode${NC}"
fi

# Feature flags (coordinator only)
ENABLE_AUDIT=${ENABLE_AUDIT:-true}
ENABLE_HA=${ENABLE_HA:-false}
ENABLE_DB2=${ENABLE_DB2:-true}

COORDINATOR_FEATURES=""
if [ "$ENABLE_AUDIT" = "true" ]; then
    COORDINATOR_FEATURES="cryptographic-audit"
fi
if [ "$ENABLE_HA" = "true" ]; then
    if [ -n "$COORDINATOR_FEATURES" ]; then
        COORDINATOR_FEATURES="${COORDINATOR_FEATURES},raft-consensus"
    else
        COORDINATOR_FEATURES="raft-consensus"
    fi
fi
if [ "$ENABLE_DB2" = "true" ]; then
    if [ -n "$COORDINATOR_FEATURES" ]; then
        COORDINATOR_FEATURES="${COORDINATOR_FEATURES},odbc"
    else
        COORDINATOR_FEATURES="odbc"
    fi
fi

if [ -n "$COORDINATOR_FEATURES" ]; then
    COORD_FEATURE_FLAG="--features ${COORDINATOR_FEATURES}"
else
    COORD_FEATURE_FLAG=""
fi

echo ""

run_workspace() {
    echo -e "${BLUE}[1/3] Testing arcxa-core...${NC}"
    if [ ${#TEST_ARGS[@]} -gt 0 ]; then
        run_cargo cargo test $MODE_FLAG -p arcxa-core "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
    else
        run_cargo cargo test $MODE_FLAG -p arcxa-core "${CARGO_ARGS[@]}"
    fi
    echo -e "${GREEN}✓ arcxa-core tests complete${NC}"
    echo ""

    echo -e "${BLUE}[2/3] Testing arcxa-model-service...${NC}"
    if [ ${#TEST_ARGS[@]} -gt 0 ]; then
        run_cargo cargo test $MODE_FLAG -p arcxa-model-service "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
    else
        run_cargo cargo test $MODE_FLAG -p arcxa-model-service "${CARGO_ARGS[@]}"
    fi
    echo -e "${GREEN}✓ arcxa-model-service tests complete${NC}"
    echo ""

    echo -e "${BLUE}[3/3] Testing arcxa-coordinator...${NC}"
    if [ -n "$COORD_FEATURE_FLAG" ]; then
        if [ ${#TEST_ARGS[@]} -gt 0 ]; then
            run_cargo cargo test $MODE_FLAG -p arcxa-coordinator $COORD_FEATURE_FLAG "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
        else
            run_cargo cargo test $MODE_FLAG -p arcxa-coordinator $COORD_FEATURE_FLAG "${CARGO_ARGS[@]}"
        fi
    else
        if [ ${#TEST_ARGS[@]} -gt 0 ]; then
            run_cargo cargo test $MODE_FLAG -p arcxa-coordinator "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
        else
            run_cargo cargo test $MODE_FLAG -p arcxa-coordinator "${CARGO_ARGS[@]}"
        fi
    fi
    echo -e "${GREEN}✓ arcxa-coordinator tests complete${NC}"
    echo ""
}

run_shard() {
    echo -e "${BLUE}Testing arcxa-shard (separate crate)...${NC}"
    pushd arcxa-shard > /dev/null
    if [ ${#TEST_ARGS[@]} -gt 0 ]; then
        run_cargo cargo test $MODE_FLAG "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
    else
        run_cargo cargo test $MODE_FLAG "${CARGO_ARGS[@]}"
    fi
    popd > /dev/null
    echo -e "${GREEN}✓ arcxa-shard tests complete${NC}"
    echo ""
}

case "$TARGET" in
    workspace)
        run_workspace
        ;;
    core)
        if [ ${#TEST_ARGS[@]} -gt 0 ]; then
            run_cargo cargo test $MODE_FLAG -p arcxa-core "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
        else
            run_cargo cargo test $MODE_FLAG -p arcxa-core "${CARGO_ARGS[@]}"
        fi
        ;;
    coordinator)
        if [ -n "$COORD_FEATURE_FLAG" ]; then
            if [ ${#TEST_ARGS[@]} -gt 0 ]; then
                run_cargo cargo test $MODE_FLAG -p arcxa-coordinator $COORD_FEATURE_FLAG "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
            else
                run_cargo cargo test $MODE_FLAG -p arcxa-coordinator $COORD_FEATURE_FLAG "${CARGO_ARGS[@]}"
            fi
        else
            if [ ${#TEST_ARGS[@]} -gt 0 ]; then
                run_cargo cargo test $MODE_FLAG -p arcxa-coordinator "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
            else
                run_cargo cargo test $MODE_FLAG -p arcxa-coordinator "${CARGO_ARGS[@]}"
            fi
        fi
        ;;
    model-service)
        if [ ${#TEST_ARGS[@]} -gt 0 ]; then
            run_cargo cargo test $MODE_FLAG -p arcxa-model-service "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]}"
        else
            run_cargo cargo test $MODE_FLAG -p arcxa-model-service "${CARGO_ARGS[@]}"
        fi
        ;;
    shard)
        run_shard
        ;;
    all)
        run_workspace
        run_shard
        ;;
    *)
        echo -e "${RED}Unknown target: $TARGET${NC}"
        show_help
        exit 1
        ;;
esac

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Tests Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
