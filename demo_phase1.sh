#!/bin/bash
# Phase 1 Field Mapping Engine - Interactive Demo
# This script demonstrates the complete workflow of the mapping engine

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
BASE_URL="http://localhost:8080"
MAPPING_API="${BASE_URL}/api/v1/mapping"

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Phase 1 Field Mapping Engine Demo                        ║${NC}"
echo -e "${BLUE}║  Statistical Matcher (TF-IDF + N-grams)                    ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if coordinator is running
echo -e "${YELLOW}[1/5] Checking coordinator health...${NC}"
if curl -sf "${BASE_URL}/health" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Coordinator is running${NC}"
else
    echo -e "❌ Coordinator is not running. Please start it first:"
    echo -e "   ./target/release/arcxa-coordinator --rest-port 8080"
    exit 1
fi

# Check mapping engine health
echo ""
echo -e "${YELLOW}[2/5] Checking mapping engine availability...${NC}"
HEALTH=$(curl -s "${MAPPING_API}/health")
STATUS=$(echo "$HEALTH" | grep -o '"status":"[^"]*"' | cut -d'"' -f4)

if [ "$STATUS" = "available" ]; then
    echo -e "${GREEN}✓ Mapping engine is available${NC}"
    echo "$HEALTH" | jq '.' 2>/dev/null || echo "$HEALTH"
else
    echo -e "❌ Mapping engine is not available"
    echo "$HEALTH"
    exit 1
fi

# Analyze a sample schema
echo ""
echo -e "${YELLOW}[3/5] Analyzing customer schema...${NC}"
cat > /tmp/analyze_request.json <<'EOF'
{
  "source_id": "postgres_demo",
  "table_name": "customers",
  "fields": [
    {
      "name": "customer_id",
      "data_type": "INTEGER",
      "nullable": false,
      "sample_values": ["1", "2", "3", "4", "5"],
      "description": "Unique customer identifier"
    },
    {
      "name": "email_address",
      "data_type": "VARCHAR",
      "nullable": false,
      "sample_values": [
        "john.doe@example.com",
        "jane.smith@company.com",
        "bob.jones@test.org"
      ]
    },
    {
      "name": "full_name",
      "data_type": "VARCHAR",
      "nullable": true,
      "sample_values": [
        "John Doe",
        "Jane Smith",
        "Bob Jones"
      ]
    },
    {
      "name": "phone_number",
      "data_type": "VARCHAR",
      "nullable": true,
      "sample_values": [
        "+1-555-1234",
        "+1-555-5678",
        ""
      ]
    }
  ],
  "sample_size": 100
}
EOF

ANALYZE_RESPONSE=$(curl -s -X POST "${MAPPING_API}/analyze" \
  -H "Content-Type: application/json" \
  -d @/tmp/analyze_request.json)

echo -e "${GREEN}✓ Schema analyzed successfully${NC}"
echo ""
echo "Fields analyzed:"
echo "$ANALYZE_RESPONSE" | jq '.fields[] | {name: .name, inferred_type: .features.inferred_type, is_primary_key: .features.context.is_primary_key}' 2>/dev/null || echo "$ANALYZE_RESPONSE"

# Extract field IDs for next step
FIELD_IDS=$(echo "$ANALYZE_RESPONSE" | jq -r '.fields[].id' 2>/dev/null)

# Get mapping candidates for each field
echo ""
echo -e "${YELLOW}[4/5] Getting mapping candidates...${NC}"

if [ -n "$FIELD_IDS" ]; then
    for FIELD_ID in $FIELD_IDS; do
        FIELD_NAME=$(echo "$ANALYZE_RESPONSE" | jq -r ".fields[] | select(.id==\"$FIELD_ID\") | .name" 2>/dev/null)
        echo ""
        echo -e "${BLUE}Field: ${FIELD_NAME}${NC}"

        CANDIDATES=$(curl -s "${MAPPING_API}/fields/${FIELD_ID}/candidates?top_k=3&min_confidence=0.1")

        if [ $? -eq 0 ]; then
            echo "$CANDIDATES" | jq '.candidates[] | {
                ontology_term: .ontology_term_uri,
                confidence: .confidence,
                explanation: .explanation
            }' 2>/dev/null || echo "$CANDIDATES"
        else
            echo "  (Failed to get candidates)"
        fi
    done
else
    echo "  No field IDs found in analysis response"
fi

# Record feedback example
echo ""
echo -e "${YELLOW}[5/5] Recording user feedback...${NC}"

# Get the first field ID (email_address)
EMAIL_FIELD_ID=$(echo "$FIELD_IDS" | head -n 2 | tail -n 1)

if [ -n "$EMAIL_FIELD_ID" ]; then
    cat > /tmp/feedback_request.json <<EOF
{
  "field_id": "${EMAIL_FIELD_ID}",
  "selected_term_uri": "http://schema.org/email",
  "accepted_top_suggestion": true,
  "user_id": "demo_user",
  "notes": "Correct mapping - customer email addresses",
  "timestamp": $(date +%s)
}
EOF

    FEEDBACK_RESPONSE=$(curl -s -X POST "${MAPPING_API}/feedback" \
      -H "Content-Type: application/json" \
      -d @/tmp/feedback_request.json)

    echo -e "${GREEN}✓ Feedback recorded${NC}"
    echo "$FEEDBACK_RESPONSE" | jq '.' 2>/dev/null || echo "$FEEDBACK_RESPONSE"
else
    echo "  (Skipping - no email field found)"
fi

# Summary
echo ""
echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Demo Complete!                                            ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "What was demonstrated:"
echo "  ✓ Health check of mapping engine"
echo "  ✓ Schema analysis with feature extraction"
echo "  ✓ Semantic pattern detection (email, phone)"
echo "  ✓ Primary key identification"
echo "  ✓ Ontology term candidate generation"
echo "  ✓ Confidence scoring (TF-IDF + N-grams)"
echo "  ✓ User feedback recording"
echo ""
echo "Key capabilities:"
echo "  • Fuzzy matching (typo tolerance)"
echo "  • Statistical profiling (distinct counts, null rates)"
echo "  • Pattern recognition (8 types: email, phone, SSN, etc.)"
echo "  • Intelligent tokenization (camelCase, snake_case)"
echo ""
echo "Next steps:"
echo "  • Try with your own database schemas"
echo "  • Experiment with different field names"
echo "  • Test fuzzy matching with typos"
echo "  • Review confidence scores and explanations"
echo ""
echo "Documentation: ./PHASE1_DEMO.md"
echo "Integration tests: cargo test --test mapping_integration_test"
echo ""
