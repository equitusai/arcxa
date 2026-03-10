#!/bin/bash
# Quick runner for E2E ETL Row Lineage Test

set -e

cd "$(dirname "$0")"

echo "========================================="
echo "Graphica E2E ETL Row Lineage Test Runner"
echo "========================================="
echo ""

# Check if Python 3 is available
if ! command -v python3 &> /dev/null; then
    echo "❌ Python 3 is required but not found"
    exit 1
fi

# Check if coordinator is running
GRAPHICA_URL="${GRAPHICA_URL:-http://localhost:8080}"
echo "🔍 Checking if Graphica coordinator is running at $GRAPHICA_URL..."

if ! curl -s -f "$GRAPHICA_URL/health" > /dev/null 2>&1; then
    echo "❌ Graphica coordinator is not responding at $GRAPHICA_URL"
    echo "   Please start the coordinator first:"
    echo "   ./start_coordinator.sh"
    exit 1
fi

echo "✅ Coordinator is running"
echo ""

# Install dependencies if needed
if ! python3 -c "import requests" 2>/dev/null; then
    echo "📦 Installing Python dependencies..."
    pip3 install -q -r requirements-e2e.txt
    echo "✅ Dependencies installed"
    echo ""
fi

# Run the test
echo "🚀 Running E2E ETL Row Lineage Test..."
echo ""

python3 e2e_etl_row_lineage_test.py

echo ""
echo "========================================="
echo "✅ Test completed!"
echo "========================================="
