#!/bin/bash
echo "=== Production unwrap() Calls ==="
rg "\.unwrap\(\)" \
  --type rust \
  --line-number \
  --no-heading \
  graphica-coordinator/src/mapping/loader/orchestration/ \
  graphica-core/src/orchestration/workflow/ \
  2>/dev/null | grep -v "test_" \
  | grep -v "#\[test\]" \
  | grep -v "#\[tokio::test\]" \
  > unwrap_inventory_prod.txt

echo "Found $(wc -l < unwrap_inventory_prod.txt) production unwrap() calls"

echo "=== Test unwrap() Calls ==="
rg "\.unwrap\(\)" \
  --type rust \
  --line-number \
  --no-heading \
  graphica-coordinator/src/mapping/loader/orchestration/ \
  graphica-core/src/orchestration/workflow/ \
  2>/dev/null | grep -E "(test_|#\[test\]|#\[tokio::test\])" \
  > unwrap_inventory_test.txt

echo "Found $(wc -l < unwrap_inventory_test.txt) test unwrap() calls"

echo ""
echo "=== Summary ==="
echo "Production unwrap(): $(wc -l < unwrap_inventory_prod.txt)"
echo "Test unwrap(): $(wc -l < unwrap_inventory_test.txt)"
echo "Total: $(( $(wc -l < unwrap_inventory_prod.txt) + $(wc -l < unwrap_inventory_test.txt) ))"
