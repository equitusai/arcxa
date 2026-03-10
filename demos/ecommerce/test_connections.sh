#!/bin/bash
# Test script to verify all demo services are accessible

echo "==================================="
echo "Testing Demo Environment"
echo "==================================="

# Test CRM Database
echo -e "\n1. Testing CRM Database (port 5434)..."
psql postgresql://graphica:graphica_demo_2024@localhost:5434/crm_db -c "SELECT COUNT(*) as customer_count FROM customers;" 2>&1 | head -5

# Test Transactions Database
echo -e "\n2. Testing Transactions Database (port 5433)..."
psql postgresql://graphica:graphica_demo_2024@localhost:5433/transactions_db -c "SELECT COUNT(*) as transaction_count FROM transactions;" 2>&1 | head -5

# Test ML Services
echo -e "\n3. Testing Duplicate Detector (port 8001)..."
curl -s http://localhost:8001/health 2>&1 | head -3

echo -e "\n4. Testing Gender Predictor (port 8002)..."
curl -s http://localhost:8002/health 2>&1 | head -3

echo -e "\n5. Testing Address Validator (port 8003)..."
curl -s http://localhost:8003/health 2>&1 | head -3

# Test ML Service - Gender Prediction
echo -e "\n6. Testing Gender Prediction API..."
curl -s -X POST http://localhost:8002/api/v1/predict \
  -H "Content-Type: application/json" \
  -d '{"first_name": "Jennifer"}' 2>&1

echo -e "\n\n==================================="
echo "✓ Connection tests complete"
echo "==================================="
