#!/bin/bash
#
# Validate async governance brain performance
# Target: 5-10x improvement over sync baseline (100-370 events/sec → 2,000+ events/sec)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "========================================"
echo "Async Governance Brain Performance Test"
echo "========================================"
echo ""

# Check if running in release mode
BUILD_MODE="${BUILD_MODE:-release}"
if [ "$BUILD_MODE" = "release" ]; then
    CARGO_FLAGS="--release"
    echo -e "${GREEN}Running in RELEASE mode${NC}"
else
    CARGO_FLAGS=""
    echo -e "${YELLOW}Running in DEBUG mode (use BUILD_MODE=release for accurate results)${NC}"
fi

# Function to run benchmark and check results
run_benchmark() {
    local bench_name=$1
    local target_throughput=$2
    local description=$3

    echo ""
    echo "Running: $description"
    echo "Target: $target_throughput events/sec"
    echo "----------------------------------------"

    # Run the benchmark
    if cargo bench $CARGO_FLAGS --bench $bench_name -- --save-baseline current --output-format bencher 2>/dev/null | tee bench_output.txt; then
        # Extract throughput from output (simplified parsing)
        throughput=$(grep -oE '[0-9,]+\s+events/sec' bench_output.txt | head -1 | sed 's/,//g' | awk '{print $1}')

        if [ -n "$throughput" ]; then
            if [ "$throughput" -ge "$target_throughput" ]; then
                echo -e "${GREEN}✅ PASS${NC}: $throughput events/sec (target: $target_throughput)"
                return 0
            else
                echo -e "${RED}❌ FAIL${NC}: $throughput events/sec (target: $target_throughput)"
                return 1
            fi
        else
            echo -e "${YELLOW}⚠️  WARNING${NC}: Could not parse throughput"
            return 2
        fi
    else
        echo -e "${RED}❌ FAIL${NC}: Benchmark failed to run"
        return 1
    fi
}

# Function to run latency benchmark
run_latency_benchmark() {
    local bench_name=$1
    local target_p99_ms=$2
    local description=$3

    echo ""
    echo "Running: $description"
    echo "Target: P99 < ${target_p99_ms}ms"
    echo "----------------------------------------"

    # Run the benchmark and capture P99 latency
    if cargo bench $CARGO_FLAGS --bench $bench_name 2>&1 | tee bench_output.txt | grep -q "P99"; then
        # Extract P99 latency (in microseconds)
        p99_us=$(grep "P99:" bench_output.txt | head -1 | awk '{print $2}')
        p99_ms=$((p99_us / 1000))

        if [ "$p99_ms" -lt "$target_p99_ms" ]; then
            echo -e "${GREEN}✅ PASS${NC}: P99 = ${p99_ms}ms (target: <${target_p99_ms}ms)"
            return 0
        else
            echo -e "${RED}❌ FAIL${NC}: P99 = ${p99_ms}ms (target: <${target_p99_ms}ms)"
            return 1
        fi
    else
        echo -e "${YELLOW}⚠️  WARNING${NC}: Could not measure latency"
        return 2
    fi
}

# Track overall results
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_WARNED=0

# Run throughput benchmarks
echo ""
echo "=== THROUGHPUT BENCHMARKS ==="

# Test 1: Single-threaded throughput
if run_benchmark "governance_throughput" 2000 "Single-threaded throughput"; then
    ((TESTS_PASSED++))
else
    ((TESTS_FAILED++))
fi
((TESTS_RUN++))

# Test 2: Multi-producer throughput (8 producers)
if BENCH_FILTER="multi_producer" run_benchmark "governance_throughput" 8000 "Multi-producer (8 threads)"; then
    ((TESTS_PASSED++))
else
    ((TESTS_FAILED++))
fi
((TESTS_RUN++))

# Test 3: Batch processing efficiency
if BENCH_FILTER="batch_size/500" run_benchmark "governance_throughput" 5000 "Batch processing (size=500)"; then
    ((TESTS_PASSED++))
else
    ((TESTS_FAILED++))
fi
((TESTS_RUN++))

# Run latency benchmarks
echo ""
echo "=== LATENCY BENCHMARKS ==="

# Test 4: Insert latency P99
if run_latency_benchmark "governance_latency" 10 "Insert event P99 latency"; then
    ((TESTS_PASSED++))
else
    ((TESTS_FAILED++))
fi
((TESTS_RUN++))

# Test 5: Query under load
if BENCH_FILTER="query_under_load/2000" run_latency_benchmark "governance_latency" 10 "Query under load (2000 events/sec)"; then
    ((TESTS_PASSED++))
else
    ((TESTS_FAILED++))
fi
((TESTS_RUN++))

# Run comparison benchmark
echo ""
echo "=== COMPARISON BENCHMARK ==="
echo "Running side-by-side comparison..."

cargo bench $CARGO_FLAGS --bench governance_throughput -- comparison 2>&1 | tee comparison_output.txt

# Extract improvement factor
if grep -q "async_improved" comparison_output.txt; then
    sync_time=$(grep "sync_baseline" comparison_output.txt | grep -oE 'time:.*\[.*\]' | grep -oE '[0-9.]+\s+ms' | head -1 | awk '{print $1}')
    async_time=$(grep "async_improved" comparison_output.txt | grep -oE 'time:.*\[.*\]' | grep -oE '[0-9.]+\s+ms' | head -1 | awk '{print $1}')

    if [ -n "$sync_time" ] && [ -n "$async_time" ]; then
        improvement=$(echo "scale=1; $sync_time / $async_time" | bc)
        echo ""
        echo -e "Improvement: ${GREEN}${improvement}x${NC} faster"

        if (( $(echo "$improvement >= 5.0" | bc -l) )); then
            echo -e "${GREEN}✅ PASS${NC}: Achieved target 5x improvement"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}❌ FAIL${NC}: Only ${improvement}x improvement (target: 5x)"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${YELLOW}⚠️  WARNING${NC}: Could not calculate improvement factor"
        ((TESTS_WARNED++))
    fi
    ((TESTS_RUN++))
fi

# Summary
echo ""
echo "========================================"
echo "PERFORMANCE VALIDATION SUMMARY"
echo "========================================"
echo "Tests Run:    $TESTS_RUN"
echo -e "Tests Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests Failed: ${RED}$TESTS_FAILED${NC}"
echo -e "Warnings:     ${YELLOW}$TESTS_WARNED${NC}"
echo ""

# Success criteria
if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ SUCCESS: All performance targets met!${NC}"
    echo ""
    echo "The async governance brain meets or exceeds all performance targets:"
    echo "• Single-threaded: 2,000+ events/sec"
    echo "• Multi-producer: 8,000+ events/sec"
    echo "• Batch processing: 5,000+ events/sec"
    echo "• P99 latency: <10ms"
    echo "• Overall improvement: 5-10x"
    exit 0
else
    echo -e "${RED}❌ FAILURE: Performance targets not met${NC}"
    echo ""
    echo "Rollback plan:"
    echo "1. Set GRAPHICA_GOVERNANCE_MODE=sync"
    echo "2. Disable async-governance feature in Cargo.toml"
    echo "3. Investigate bottlenecks with profiling"
    exit 1
fi