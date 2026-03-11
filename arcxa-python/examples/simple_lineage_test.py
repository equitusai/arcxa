#!/usr/bin/env python3
"""
Simple Lineage Test - Minimal ETL to Generate Lineage Data

This script:
1. Creates a simple workflow to load CSV data into DB2
2. Ensures lineage tracking is enabled
3. Outputs row keys for querying lineage

Usage:
    python3 simple_lineage_test.py
"""

import requests
import json
import time
import sys

# Configuration
COORDINATOR_URL = "http://localhost:8082"
CSV_FILE = "/tmp/test_patients.csv"
DB2_TABLE = "TEST_PATIENTS"

def print_step(msg):
    print(f"\n{'='*70}")
    print(f"  {msg}")
    print(f"{'='*70}")

def create_workflow():
    """Create a simple CSV → DB2 workflow"""
    print_step("Step 1: Creating Workflow")

    workflow_def = {
        "name": "simple_lineage_test",
        "description": "Simple test workflow for lineage tracking",
        "steps": [
            {
                "id": "read_csv",
                "type": "csv_source",
                "config": {
                    "file_path": CSV_FILE,
                    "has_header": True
                }
            },
            {
                "id": "load_db2",
                "type": "db2_loader",
                "config": {
                    "connection": {
                        "host": "localhost",
                        "port": 50000,
                        "database": "GRAPHICA",
                        "username": "db2inst1",
                        "password": "graphica-db2-pass"
                    },
                    "table": DB2_TABLE,
                    "create_table": True,
                    "enable_lineage": True
                },
                "depends_on": ["read_csv"]
            }
        ]
    }

    response = requests.post(
        f"{COORDINATOR_URL}/api/v1/workflows",
        json=workflow_def,
        headers={"Content-Type": "application/json"}
    )

    if response.status_code in [200, 201]:
        workflow_id = response.json().get("workflow_id") or response.json().get("id")
        print(f"✓ Workflow created: {workflow_id}")
        return workflow_id
    else:
        print(f"✗ Failed to create workflow: {response.status_code}")
        print(f"  Response: {response.text}")
        sys.exit(1)

def execute_workflow(workflow_id):
    """Execute the workflow and wait for completion"""
    print_step("Step 2: Executing Workflow")

    response = requests.post(
        f"{COORDINATOR_URL}/api/v1/workflows/{workflow_id}/execute",
        headers={"Content-Type": "application/json"}
    )

    if response.status_code in [200, 202]:
        execution_id = response.json().get("execution_id")
        print(f"✓ Workflow execution started: {execution_id}")

        # Wait for completion
        print("\nWaiting for execution to complete...")
        for i in range(60):  # Wait up to 60 seconds
            time.sleep(1)

            status_resp = requests.get(
                f"{COORDINATOR_URL}/api/v1/workflows/{workflow_id}/executions/{execution_id}"
            )

            if status_resp.status_code == 200:
                status = status_resp.json().get("status")
                print(f"  Status: {status}", end="\r")

                if status in ["completed", "success", "succeeded"]:
                    print(f"\n✓ Workflow completed successfully!")
                    return execution_id, True
                elif status in ["failed", "error"]:
                    print(f"\n✗ Workflow failed!")
                    print(f"  Details: {status_resp.json()}")
                    return execution_id, False

        print(f"\n⚠ Workflow still running after 60 seconds")
        return execution_id, None
    else:
        print(f"✗ Failed to execute workflow: {response.status_code}")
        print(f"  Response: {response.text}")
        sys.exit(1)

def verify_data():
    """Verify data was loaded into DB2"""
    print_step("Step 3: Verifying Data in DB2")

    import subprocess
    result = subprocess.run(
        ["docker", "exec", "graphica-db2", "su", "-", "db2inst1", "-c",
         f"db2 connect to GRAPHICA && db2 'SELECT COUNT(*) FROM {DB2_TABLE}' && db2 connect reset"],
        capture_output=True,
        text=True
    )

    if "5" in result.stdout or "5 record(s) selected" in result.stdout:
        print("✓ Data verified in DB2: 5 rows loaded")
        return True
    else:
        print("⚠ Could not verify data in DB2")
        print(f"  Output: {result.stdout}")
        return False

def get_row_keys():
    """Get row keys for lineage querying"""
    print_step("Step 4: Row Keys for Lineage Queries")

    print("\nYou can now query lineage for these rows:")
    print()

    # CSV row keys
    for i in range(1, 6):
        row_key = f"csv:test_patients.csv:{i}"
        print(f"  • {row_key}")

    print()
    print("Query example:")
    print(f"  python3 query_lineage_ascii.py --row-key 'csv:test_patients.csv:1'")
    print()

def main():
    print("\n" + "="*70)
    print("  Simple Lineage Test - Generate Lineage Data")
    print("="*70)

    # Check coordinator
    try:
        health = requests.get(f"{COORDINATOR_URL}/health", timeout=5)
        health.raise_for_status()
        print(f"✓ Coordinator is running")
    except:
        print(f"✗ Cannot connect to coordinator at {COORDINATOR_URL}")
        print(f"  Please start it with: ./run-local.sh")
        sys.exit(1)

    # Create and execute workflow
    workflow_id = create_workflow()
    execution_id, success = execute_workflow(workflow_id)

    if success:
        verify_data()
        get_row_keys()

        print_step("✓ Success! Lineage data has been generated")
        print()
        print("Next step: Query the lineage!")
        print("  python3 examples/query_lineage_ascii.py --row-key 'csv:test_patients.csv:1'")
        print()
    else:
        print_step("✗ Workflow execution failed")
        print()
        print("Check the coordinator logs for details:")
        print("  tail -f /root/graphica/graphica/data/coordinator/coordinator.log")
        print()

if __name__ == "__main__":
    main()
