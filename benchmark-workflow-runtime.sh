#!/bin/bash
# Run the canonical workflow-runtime benchmark harness in a Conda-safe environment.

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

RUST_TOOLCHAIN="stable"
PROFILE="${ARCXA_WORKFLOW_BENCH_PROFILE:-quick}"
NO_RUN="false"
CHECK_ONLY="false"
EXTRA_ARGS=()

run_cargo() {
    local cargo_args=("$@")
    if [ "${cargo_args[0]}" = "cargo" ]; then
        cargo_args=("${cargo_args[@]:1}")
    fi

    env -u CC \
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

show_help() {
    echo "Usage: ./benchmark-workflow-runtime.sh [quick|baseline] [--check-only|--no-run] [-- <criterion args>]"
    echo ""
    echo "Profiles:"
    echo "  quick      Run the smaller row-count set (default)"
    echo "  baseline   Include the 1M-row baseline case"
    echo ""
    echo "Examples:"
    echo "  ./benchmark-workflow-runtime.sh"
    echo "  ./benchmark-workflow-runtime.sh baseline"
    echo "  ./benchmark-workflow-runtime.sh --check-only"
    echo "  ./benchmark-workflow-runtime.sh --no-run"
    echo "  ./benchmark-workflow-runtime.sh baseline -- --sample-size 10"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        quick|baseline)
            PROFILE="$1"
            shift
            ;;
        --no-run)
            NO_RUN="true"
            shift
            ;;
        --check-only)
            CHECK_ONLY="true"
            shift
            ;;
        --help)
            show_help
            exit 0
            ;;
        --)
            shift
            EXTRA_ARGS+=("$@")
            break
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Workflow Runtime Benchmark Harness${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${YELLOW}Profile: ${PROFILE}${NC}"
echo -e "${YELLOW}Using repo-managed Conda-safe cargo wrapper${NC}"
echo ""

if [ "$CHECK_ONLY" = "true" ]; then
    BENCH_CMD=(cargo check -p arcxa-core --bench workflow_runtime_bench)
    if [ "$PROFILE" = "quick" ]; then
        BENCH_CMD+=(--profile bench-quick)
    fi
else
    BENCH_CMD=(cargo bench -p arcxa-core --bench workflow_runtime_bench)
    if [ "$PROFILE" = "quick" ]; then
        BENCH_CMD+=(--profile bench-quick)
    fi
    if [ "$NO_RUN" = "true" ]; then
        BENCH_CMD+=(--no-run)
    fi
    if [ ${#EXTRA_ARGS[@]} -gt 0 ]; then
        BENCH_CMD+=(-- "${EXTRA_ARGS[@]}")
    fi
fi

echo -e "${BLUE}Running workflow runtime benchmark harness...${NC}"
ARCXA_WORKFLOW_BENCH_PROFILE="${PROFILE}" run_cargo "${BENCH_CMD[@]}"
echo ""
echo -e "${GREEN}Workflow runtime benchmark harness complete${NC}"
echo -e "${GREEN}Benchmark results are printed above; Criterion artifacts may be emitted under target/ for the active profile${NC}"
