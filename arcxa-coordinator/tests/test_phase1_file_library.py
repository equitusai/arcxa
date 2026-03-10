#!/usr/bin/env python3
"""
Phase 1 Test: File Library Upload
Tests the upload_to_file_library method
"""

import sys
import os

# Import the main test class
from test_e2e_csv_ontology_db2_lineage import GraphicaETLTest

def test_file_library_upload():
    """Test file library upload functionality"""
    print("=" * 80)
    print("PHASE 1 TEST: File Library Upload")
    print("=" * 80)

    test = GraphicaETLTest(base_url="http://localhost:8080")

    # Step 1: Generate a small CSV
    print("\n[1/3] Generating test CSV...")
    customers = test.generate_customers_csv(num_rows=10)
    csv_path = test.csv_files["customers"]
    print(f"   ✅ CSV created: {csv_path}")

    # Step 2: Upload to file library
    print("\n[2/3] Uploading to File Library...")
    file_id = test.upload_to_file_library(csv_path, "CUSTOMERS")

    if not file_id:
        print("\n❌ PHASE 1 FAILED: File library upload returned None")
        return False

    print(f"\n   ✅ File uploaded with ID: {file_id}")

    # Step 3: Verify file exists
    print("\n[3/3] Verifying file in library...")
    verify_url = f"{test.base_url}/api/v1/file-library/files/{file_id}"

    try:
        response = test.session.get(verify_url, timeout=10)
        if response.status_code == 200:
            file_info = response.json()
            print(f"   ✅ File verified:")
            print(f"      - Filename: {file_info.get('filename', 'N/A')}")
            print(f"      - Size: {file_info.get('size_bytes', 0)} bytes")
            print(f"      - Uploaded: {file_info.get('uploaded_at', 'N/A')}")
            print(f"\n✅ PHASE 1 PASSED: File library upload working correctly!")
            return True
        else:
            print(f"   ❌ File verification failed: {response.status_code}")
            print(f"   Response: {response.text}")
            return False

    except Exception as e:
        print(f"   ❌ Verification error: {e}")
        return False

if __name__ == "__main__":
    success = test_file_library_upload()
    sys.exit(0 if success else 1)
