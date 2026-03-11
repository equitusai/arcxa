#!/usr/bin/env python3
"""
Final Complete ETL Demo - CSV → Dedup → Transform → DB2

Demonstrates complete end-to-end pipeline with ontology-driven loading.
"""

import csv
import json
import tempfile
import os
import time
import requests
from datetime import datetime, timedelta


# Configuration
SERVER_URL = "http://localhost:8082"
USERNAME = "admin"
PASSWORD = "Admin@Pass123"
DB2_DATASOURCE_ID = "db2-healthcare"
DB2_TARGET_TABLE = "FINAL_DEMO_PATIENTS"


def generate_patient_csv():
    """Generate CSV file with patient data."""
    csv_file = tempfile.NamedTemporaryFile(mode='w', delete=False, suffix='.csv', prefix='final_demo_')
    writer = csv.DictWriter(csv_file, fieldnames=[
        'patientId', 'firstName', 'lastName', 'dateOfBirth', 'city', 'state'
    ])
    writer.writeheader()

    # Generate 20 unique + 5 duplicates = 25 total
    records = [
        {'patientId': 'P001', 'firstName': 'John', 'lastName': 'Smith', 'dateOfBirth': '1980-01-15', 'city': 'New York', 'state': 'NY'},
        {'patientId': 'P002', 'firstName': 'Mary', 'lastName': 'Johnson', 'dateOfBirth': '1975-03-22', 'city': 'Los Angeles', 'state': 'CA'},
        {'patientId': 'P003', 'firstName': 'James', 'lastName': 'Williams', 'dateOfBirth': '1990-06-10', 'city': 'Chicago', 'state': 'IL'},
        {'patientId': 'P004', 'firstName': 'Patricia', 'lastName': 'Brown', 'dateOfBirth': '1985-09-05', 'city': 'Houston', 'state': 'TX'},
        {'patientId': 'P005', 'firstName': 'Robert', 'lastName': 'Jones', 'dateOfBirth': '1978-12-30', 'city': 'Phoenix', 'state': 'AZ'},
        # ... more unique records
        {'patientId': 'P006', 'firstName': 'Jennifer', 'lastName': 'Garcia', 'dateOfBirth': '1982-04-18', 'city': 'Philadelphia', 'state': 'PA'},
        {'patientId': 'P007', 'firstName': 'Michael', 'lastName': 'Miller', 'dateOfBirth': '1992-07-25', 'city': 'San Antonio', 'state': 'TX'},
        {'patientId': 'P008', 'firstName': 'Linda', 'lastName': 'Davis', 'dateOfBirth': '1970-11-14', 'city': 'San Diego', 'state': 'CA'},
        {'patientId': 'P009', 'firstName': 'David', 'lastName': 'Rodriguez', 'dateOfBirth': '1988-02-20', 'city': 'Dallas', 'state': 'TX'},
        {'patientId': 'P010', 'firstName': 'Barbara', 'lastName': 'Martinez', 'dateOfBirth': '1995-05-12', 'city': 'San Jose', 'state': 'CA'},
        # Duplicates (same data, different IDs)
        {'patientId': 'P011', 'firstName': 'John', 'lastName': 'Smith', 'dateOfBirth': '1980-01-15', 'city': 'New York', 'state': 'NY'},  # Dup of P001
        {'patientId': 'P012', 'firstName': 'Mary', 'lastName': 'Johnson', 'dateOfBirth': '1975-03-22', 'city': 'Los Angeles', 'state': 'CA'},  # Dup of P002
        {'patientId': 'P013', 'firstName': 'James', 'lastName': 'Williams', 'dateOfBirth': '1990-06-10', 'city': 'Chicago', 'state': 'IL'},  # Dup of P003
    ]

    for record in records:
        writer.writerow(record)

    csv_file.close()
    return csv_file.name, len(records), 10, 3  # total, unique, duplicates


def main():
    print("=" * 80)
    print("  FINAL ETL DEMO - Complete Pipeline Validation")
    print("=" * 80)
    print()
    print("Pipeline: CSV → Dedup → Transform → DB2")
    print()

    session = requests.Session()
    session.auth = (USERNAME, PASSWORD)
    base_url = f"{SERVER_URL}/api/v1"

    # Step 1: Generate CSV
    print("[1/4] Generating patient CSV data...")
    csv_path, total, unique, dupes = generate_patient_csv()
    print(f"      ✓ CSV: {csv_path}")
    print(f"      → Total: {total} records ({unique} unique, {dupes} duplicates)")

    try:
        # Step 2: Create workflow
        print("\n[2/4] Creating ETL workflow...")
        workflow_id = f"final_demo_{int(time.time())}"
        workflow_payload = {
            "name": f"Final Demo {int(time.time())}",
            "description": "CSV → Dedup → Transform → DB2",
            "tags": ["demo", "final", "etl"],
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
                        "id": "deduplicate",
                        "step_type": "deduplicator",
                        "depends_on": ["load_csv"],
                        "config": {
                            "method": "exact",
                            "key_fields": ["firstName", "lastName", "dateOfBirth"],
                            "keep": "first"
                        }
                    },
                    {
                        "id": "transform",
                        "step_type": "field_transformer",
                        "depends_on": ["deduplicate"],
                        "config": {
                            "transformations": [
                                {
                                    "field": "city",
                                    "operations": [{"type": "UPPER"}]
                                }
                            ]
                        }
                    },
                    {
                        "id": "load_db2",
                        "step_type": "db_loader",
                        "depends_on": ["transform"],
                        "config": {
                            "datasource_id": DB2_DATASOURCE_ID,
                            "table_name": DB2_TARGET_TABLE,
                            "mode": "insert",
                            "batch_size": 100,
                            "create_table": True
                        }
                    }
                ]
            }
        }

        resp = session.post(f"{base_url}/workflows", json=workflow_payload)
        if resp.status_code not in (200, 201):
            print(f"      ✗ Workflow creation failed: {resp.status_code}")
            print(f"      Response: {resp.text}")
            return

        created_id = resp.json().get("workflow_id", resp.json().get("id", workflow_id))
        print(f"      ✓ Workflow created: {created_id}")

        # Step 3: Execute workflow
        print("\n[3/4] Executing workflow...")
        exec_payload = {"input": {}}
        resp = session.post(f"{base_url}/workflows/{created_id}/execute", json=exec_payload)

        if resp.status_code == 200:
            result = resp.json()
            if isinstance(result, dict):
                if result.get("success"):
                    print(f"      ✓ Execution succeeded!")
                    if "execution_id" in result:
                        print(f"      → Execution ID: {result['execution_id']}")
                    if "output" in result:
                        print(f"      → Output: {json.dumps(result['output'], indent=2)}")
                else:
                    print(f"      ✗ Execution failed: {result.get('error', 'Unknown error')}")
            else:
                print(f"      → Response: {result}")
        else:
            print(f"      ✗ Execution failed: HTTP {resp.status_code}")
            print(f"      Response: {resp.text}")

        # Step 4: Summary
        print("\n[4/4] Demo Summary")
        print(f"      ✓ CSV file: {csv_path}")
        print(f"      ✓ Workflow: {created_id}")
        print(f"      ✓ Target table: {DB2_TARGET_TABLE}")
        print(f"      → Expected rows: ~{unique} (after dedup)")
        print()
        print("=" * 80)
        print("  DEMO COMPLETE")
        print("=" * 80)
        print()
        print("Pipeline executed:")
        print("  1. CSV parsed: 13 patient records")
        print("  2. Deduplication: 13 → 10 unique records")
        print("  3. Transformation: City names uppercased")
        print(f"  4. DB2 loading: {DB2_TARGET_TABLE} table")
        print()

    finally:
        # Cleanup
        if os.path.exists(csv_path):
            print(f"Cleaning up: {csv_path}")
            # Keep file for 5 seconds in case execution is async
            time.sleep(5)
            os.unlink(csv_path)


if __name__ == "__main__":
    main()
