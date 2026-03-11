#!/usr/bin/env python3
"""
Working Lineage Demo - Fixed to Register Datasource First

This script:
1. Registers the DB2 datasource if it doesn't exist
2. Creates a simple ETL workflow
3. Executes the workflow
4. Generates queryable lineage data

Usage:
    python3 working_lineage_demo.py
"""

import csv
import json
import tempfile
import time
import requests
from datetime import datetime

# Configuration
SERVER_URL = "http://localhost:8082"
DB2_DATASOURCE_ID = "Healthcare DB2 Database"  # Must match exact title
DB2_TARGET_TABLE = "LINEAGE_DEMO_PATIENTS"


def print_step(msg):
    print(f"\n{'='*70}")
    print(f"  {msg}")
    print(f"{'='*70}")


def generate_patient_csv():
    """Generate CSV file with patient data."""
    csv_file = tempfile.NamedTemporaryFile(
        mode='w', delete=False, suffix='.csv', prefix='lineage_demo_'
    )

    writer = csv.DictWriter(csv_file, fieldnames=[
        'patient_id', 'first_name', 'last_name', 'birth_date', 'city'
    ])

    writer.writeheader()
    patients = [
        {'patient_id': 'P001', 'first_name': 'Alice', 'last_name': 'Johnson',
         'birth_date': '1985-03-15', 'city': 'New York'},
        {'patient_id': 'P002', 'first_name': 'Bob', 'last_name': 'Smith',
         'birth_date': '1990-07-22', 'city': 'Los Angeles'},
        {'patient_id': 'P003', 'first_name': 'Carol', 'last_name': 'White',
         'birth_date': '1978-11-30', 'city': 'Chicago'},
        {'patient_id': 'P004', 'first_name': 'David', 'last_name': 'Brown',
         'birth_date': '1995-01-08', 'city': 'Houston'},
        {'patient_id': 'P005', 'first_name': 'Eve', 'last_name': 'Davis',
         'birth_date': '1982-09-12', 'city': 'Phoenix'},
    ]

    for patient in patients:
        writer.writerow(patient)

    csv_file.close()
    return csv_file.name, len(patients)


def register_datasource(session):
    """Register DB2 datasource if it doesn't exist."""
    print_step("[1/5] Checking/Registering DB2 Datasource")

    # Check if datasource exists
    try:
        resp = session.get(f"{SERVER_URL}/api/v1/datasources")
        if resp.status_code == 200:
            sources = resp.json()
            for source in sources.get("sources", []):
                if source.get("id") == DB2_DATASOURCE_ID or \
                   source.get("title") == "Healthcare DB2 Database":
                    print(f"      ✓ Datasource already exists: {DB2_DATASOURCE_ID}")
                    return True
    except Exception as e:
        print(f"      ⚠ Error checking datasources: {e}")

    # Create datasource
    print(f"      Creating datasource: {DB2_DATASOURCE_ID}")

    datasource_config = {
        "title": "Healthcare DB2 Database",
        "description": "DB2 database for lineage demo",
        "sourceType": "DB2",
        "connection": {
            "secretRef": "local://db2-creds",
            "config": {
                "type": "DB2",
                "host": "localhost",
                "port": 50000,
                "database": "GRAPHICA",
                "schema": "DB2INST1"
            },
            "encryptionEnabled": False
        },
        "tags": ["db2", "demo", "lineage"]
    }

    try:
        resp = session.post(
            f"{SERVER_URL}/api/v1/datasources",
            json=datasource_config,
            headers={"Content-Type": "application/json"}
        )

        if resp.status_code in (200, 201):
            print(f"      ✓ Datasource registered successfully")
            return True
        else:
            print(f"      ✗ Failed to register datasource: {resp.status_code}")
            print(f"      Response: {resp.text}")
            return False
    except Exception as e:
        print(f"      ✗ Error registering datasource: {e}")
        return False


def create_workflow(session, csv_path):
    """Create ETL workflow."""
    print_step("[3/5] Creating ETL Workflow")

    workflow_def = {
        "name": f"lineage_demo_{int(time.time())}",
        "description": "Simple ETL for lineage demonstration",
        "definition": {
            "steps": [
                {
                    "id": "load_csv",
                    "step_type": "csv_source",
                    "config": {
                        "file_path": csv_path,
                        "has_header": True
                    }
                },
                {
                    "id": "load_db2",
                    "step_type": "db_loader",
                    "config": {
                        "datasource_id": DB2_DATASOURCE_ID,
                        "table_name": DB2_TARGET_TABLE,
                        "mode": "insert",
                        "create_table": True
                    },
                    "depends_on": ["load_csv"]
                }
            ]
        }
    }

    try:
        resp = session.post(
            f"{SERVER_URL}/api/v1/workflows",
            json=workflow_def,
            headers={"Content-Type": "application/json"}
        )

        if resp.status_code in (200, 201):
            workflow_data = resp.json()
            workflow_id = workflow_data.get("id") or workflow_data.get("workflow_id")
            print(f"      ✓ Workflow created: {workflow_id}")
            return workflow_id
        else:
            print(f"      ✗ Failed to create workflow: {resp.status_code}")
            print(f"      Response: {resp.text}")
            return None
    except Exception as e:
        print(f"      ✗ Error creating workflow: {e}")
        return None


def execute_workflow(session, workflow_id):
    """Execute the workflow."""
    print_step("[4/5] Executing Workflow")

    try:
        resp = session.post(
            f"{SERVER_URL}/api/v1/workflows/{workflow_id}/execute",
            json={"input": {}},
            headers={"Content-Type": "application/json"}
        )

        if resp.status_code in (200, 202):
            execution_data = resp.json()
            execution_id = execution_data.get("execution_id")
            print(f"      ✓ Execution started: {execution_id}")

            # Wait for completion
            print("\n      Waiting for completion", end="")
            for i in range(30):
                time.sleep(1)
                print(".", end="", flush=True)

                status_resp = session.get(
                    f"{SERVER_URL}/api/v1/workflows/{workflow_id}/executions/{execution_id}"
                )

                if status_resp.status_code == 200:
                    status = status_resp.json().get("status")
                    if status in ["completed", "success", "succeeded"]:
                        print("\n      ✓ Workflow completed successfully!")
                        return True
                    elif status in ["failed", "error"]:
                        print(f"\n      ✗ Workflow failed with status: {status}")
                        print(f"      Details: {status_resp.json()}")
                        return False

            print("\n      ⚠ Workflow still running after 30 seconds")
            return None
        else:
            print(f"      ✗ Failed to execute workflow: {resp.status_code}")
            print(f"      Response: {resp.text}")
            return False
    except Exception as e:
        print(f"      ✗ Error executing workflow: {e}")
        return False


def verify_data(session):
    """Verify data was loaded."""
    print_step("[5/5] Verifying Data")

    import subprocess
    result = subprocess.run(
        ["docker", "exec", "graphica-db2", "su", "-", "db2inst1", "-c",
         f"db2 connect to GRAPHICA && db2 'SELECT COUNT(*) FROM {DB2_TARGET_TABLE}' && db2 connect reset"],
        capture_output=True,
        text=True
    )

    if "5" in result.stdout or "5 record(s) selected" in result.stdout:
        print("      ✓ Data verified: 5 rows in DB2")
        return True
    else:
        print("      ⚠ Could not verify data")
        print(f"      Output: {result.stdout[:200]}")
        return False


def main():
    print("\n" + "="*70)
    print("  Working Lineage Demo - With Datasource Registration")
    print("="*70)

    session = requests.Session()

    # Check coordinator
    try:
        health = session.get(f"{SERVER_URL}/health", timeout=5)
        health.raise_for_status()
        print(f"✓ Coordinator is running at {SERVER_URL}")
    except:
        print(f"✗ Cannot connect to coordinator at {SERVER_URL}")
        print(f"  Please start it with: ./run-local.sh")
        return

    # Generate CSV
    print_step("[0/5] Generating Test Data")
    csv_path, row_count = generate_patient_csv()
    print(f"      ✓ CSV created: {csv_path}")
    print(f"      → {row_count} patient records")

    # Register datasource
    if not register_datasource(session):
        print("\n✗ Failed to register datasource. Cannot continue.")
        return

    # Small delay to ensure datasource is ready
    time.sleep(1)

    # Create workflow
    workflow_id = create_workflow(session, csv_path)
    if not workflow_id:
        print("\n✗ Failed to create workflow. Cannot continue.")
        return

    # Execute workflow
    success = execute_workflow(session, workflow_id)

    if success:
        verify_data(session)

        print_step("✓ SUCCESS - Lineage Data Generated!")
        print()
        print("You can now query lineage for these rows:")
        print()
        for i in range(1, 6):
            print(f"  • csv:{csv_path.split('/')[-1]}:{i}")
        print()
        print("Query example:")
        print(f"  python3 examples/query_lineage_ascii.py --row-key 'csv:{csv_path.split('/')[-1]}:1'")
        print()
    elif success is False:
        print_step("✗ Workflow Failed")
        print()
        print("Check logs for details:")
        print("  tail -f /root/graphica/graphica/data/coordinator/coordinator.log")
        print()
    else:
        print_step("⚠ Workflow Status Unknown")
        print()
        print("Check the coordinator UI or logs for status")
        print()


if __name__ == "__main__":
    main()
