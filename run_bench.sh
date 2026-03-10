#!/bin/bash
# Run E2E ETL & Lineage Benchmark Test
#
# Usage:
#   ./run_bench.sh [admin_password]
#
# Prerequisites:
#   - Local cluster running (./run-local.sh)
#   - Build completed (cargo build --release --bin e2e_etl_lineage_bench)

set -e

ADMIN_PASSWORD="${1:-admin123}"

echo "╔═══════════════════════════════════════════════════════════╗"
echo "║  Graphica E2E ETL & Lineage Benchmark                    ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "Checking prerequisites..."

# Check if coordinator is running
if ! curl -s http://localhost:8080/health > /dev/null 2>&1; then
    echo "❌ Error: Coordinator not running at http://localhost:8080"
    echo "   Please start the cluster first:"
    echo "   $ ./run-local.sh"
    exit 1
fi

echo "✓ Coordinator is running"

# Build bench test if needed
if [ ! -f "target/release/e2e_etl_lineage_bench" ]; then
    echo "Building benchmark test..."
    cargo build --release --bin e2e_etl_lineage_bench
fi

echo ""
echo "Running benchmark with admin password: ${ADMIN_PASSWORD:0:4}****"
echo "═══════════════════════════════════════════════════════════"
echo ""

# Run the benchmark
./target/release/e2e_etl_lineage_bench "$ADMIN_PASSWORD"

exit_code=$?

if [ $exit_code -eq 0 ]; then
    echo "╔═══════════════════════════════════════════════════════════╗"
    echo "║  ✅ BENCHMARK PASSED                                      ║"
    echo "╚═══════════════════════════════════════════════════════════╝"
else
    echo "╔═══════════════════════════════════════════════════════════╗"
    echo "║  ❌ BENCHMARK FAILED                                      ║"
    echo "╚═══════════════════════════════════════════════════════════╝"
fi

exit $exit_code
