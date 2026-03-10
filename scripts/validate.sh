#!/bin/bash
# Graphica Pipeline Validation Script
# Validates correctness and data integrity

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0
WARN=0

echo "╔════════════════════════════════════════════════════════════╗"
echo "║         GRAPHICA PIPELINE VALIDATION                       ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Test 1: Server Reachability
echo -n "[Test 1/8] Server reachability... "
if curl -sf http://localhost:8080/health/live > /dev/null 2>&1; then
    echo -e "${GREEN}PASS${NC}"
    ((PASS++))
else
    echo -e "${RED}FAIL${NC} - Server not responding"
    ((FAIL++))
    exit 1
fi

# Test 2: RocksDB Health
echo -n "[Test 2/8] RocksDB storage... "
STORAGE_HEALTH=$(curl -s http://localhost:8080/health/storage 2>/dev/null | jq -r '.rocksdb // false' 2>/dev/null || echo "false")
if [ "$STORAGE_HEALTH" == "true" ]; then
    echo -e "${GREEN}PASS${NC}"
    ((PASS++))
else
    echo -e "${RED}FAIL${NC} - RocksDB not healthy"
    ((FAIL++))
fi

# Test 3: RDF Store Health
echo -n "[Test 3/8] RDF store... "
RDF_HEALTH=$(curl -s http://localhost:8080/health/storage 2>/dev/null | jq -r '.rdf_store // false' 2>/dev/null || echo "false")
if [ "$RDF_HEALTH" == "true" ]; then
    echo -e "${GREEN}PASS${NC}"
    ((PASS++))
else
    echo -e "${RED}FAIL${NC} - RDF store not healthy"
    ((FAIL++))
fi

# Test 4: Triple Count
echo -n "[Test 4/8] RDF triple count... "
TRIPLE_COUNT=$(curl -s -X POST http://localhost:8080/api/v1/governance/sparql \
    -H 'Content-Type: application/json' \
    -d '{"sparql": "SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }"}' \
    2>/dev/null | jq -r '.results[0].count // 0' 2>/dev/null || echo "0")

if [ "$TRIPLE_COUNT" -gt 200 ]; then
    echo -e "${GREEN}PASS${NC} ($TRIPLE_COUNT triples)"
    ((PASS++))
elif [ "$TRIPLE_COUNT" -gt 0 ]; then
    echo -e "${YELLOW}WARN${NC} ($TRIPLE_COUNT triples - expected >200)"
    ((WARN++))
else
    echo -e "${RED}FAIL${NC} (0 triples - nothing materialized)"
    ((FAIL++))
fi

# Test 5: Lineage Queries
echo -n "[Test 5/8] SPARQL lineage queries... "
LINEAGE_RESULTS=$(curl -s -X POST http://localhost:8080/api/v1/governance/sparql \
    -H 'Content-Type: application/json' \
    -d '{"sparql": "PREFIX gph: <http://graphica.io/ontology#> SELECT ?dataset ?recordId WHERE { ?lineage gph:dataset ?dataset . ?lineage gph:recordId ?recordId } LIMIT 10"}' \
    2>/dev/null | jq -r '.results | length' 2>/dev/null || echo "0")

if [ "$LINEAGE_RESULTS" -gt 5 ]; then
    echo -e "${GREEN}PASS${NC} ($LINEAGE_RESULTS results)"
    ((PASS++))
elif [ "$LINEAGE_RESULTS" -gt 0 ]; then
    echo -e "${YELLOW}WARN${NC} ($LINEAGE_RESULTS results - expected >5)"
    ((WARN++))
else
    echo -e "${RED}FAIL${NC} (no lineage data found)"
    ((FAIL++))
fi

# Test 6: Dataset Diversity
echo -n "[Test 6/8] Dataset diversity... "
DATASETS=$(curl -s -X POST http://localhost:8080/api/v1/governance/sparql \
    -H 'Content-Type: application/json' \
    -d '{"sparql": "PREFIX gph: <http://graphica.io/ontology#> SELECT DISTINCT ?dataset WHERE { ?lineage gph:dataset ?dataset }"}' \
    2>/dev/null | jq -r '.results | length' 2>/dev/null || echo "0")

if [ "$DATASETS" -ge 3 ]; then
    echo -e "${GREEN}PASS${NC} ($DATASETS datasets)"
    ((PASS++))
elif [ "$DATASETS" -gt 0 ]; then
    echo -e "${YELLOW}WARN${NC} ($DATASETS datasets - expected ≥3)"
    ((WARN++))
else
    echo -e "${RED}FAIL${NC} (no datasets found)"
    ((FAIL++))
fi

# Test 7: Lineage Completeness
echo -n "[Test 7/8] Lineage completeness... "
COMPLETE_LINEAGE=$(curl -s -X POST http://localhost:8080/api/v1/governance/sparql \
    -H 'Content-Type: application/json' \
    -d '{"sparql": "PREFIX prov: <http://www.w3.org/ns/prov#> SELECT ?lineage WHERE { ?lineage prov:used ?source . ?entity prov:wasGeneratedBy ?lineage } LIMIT 1"}' \
    2>/dev/null | jq -r '.results | length' 2>/dev/null || echo "0")

if [ "$COMPLETE_LINEAGE" -gt 0 ]; then
    echo -e "${GREEN}PASS${NC} (source → transform → output chain verified)"
    ((PASS++))
else
    echo -e "${YELLOW}WARN${NC} (incomplete lineage chains)"
    ((WARN++))
fi

# Test 8: Data Loss Check
echo -n "[Test 8/8] Data loss detection... "
# This is a simplified check - in production would compare Kafka offsets vs stored records
if [ "$TRIPLE_COUNT" -gt 0 ] && [ "$LINEAGE_RESULTS" -gt 0 ]; then
    echo -e "${GREEN}PASS${NC} (no obvious data loss)"
    ((PASS++))
else
    echo -e "${RED}FAIL${NC} (potential data loss detected)"
    ((FAIL++))
fi

# Summary
echo ""
echo "════════════════════════════════════════════════════════════"
echo "VALIDATION SUMMARY"
echo "════════════════════════════════════════════════════════════"
echo ""
echo -e "Tests Passed:  ${GREEN}$PASS${NC}"
echo -e "Tests Failed:  ${RED}$FAIL${NC}"
echo -e "Warnings:      ${YELLOW}$WARN${NC}"
echo ""

# Overall result
if [ $FAIL -eq 0 ] && [ $WARN -eq 0 ]; then
    echo -e "${GREEN}✓ VALIDATION PASSED - All systems operational${NC}"
    exit 0
elif [ $FAIL -eq 0 ]; then
    echo -e "${YELLOW}⚠ VALIDATION PASSED WITH WARNINGS - System functional but degraded${NC}"
    exit 0
else
    echo -e "${RED}✗ VALIDATION FAILED - Critical issues detected${NC}"
    exit 1
fi
