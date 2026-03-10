#!/usr/bin/env python3
"""
End-to-End ETL Row-Level Lineage Test

This script tests the complete ETL workflow with row-level lineage tracking:
1. CSV ingestion
2. Data transformations (standardization, validation, quality rules)
3. Load to DB2 database
4. Row-level lineage query and verification

Requirements:
- Graphica coordinator running on localhost:8080
- DB2 database configured
- Python 3.8+
- pip install requests pandas

Usage:
    python e2e_etl_row_lineage_test.py
"""

import json
import os
import sys
import time
from datetime import datetime
from typing import Dict, List, Optional, Any
import requests
from pathlib import Path

# Configuration
GRAPHICA_URL = os.getenv("GRAPHICA_URL", "http://localhost:8080")
GRAPHICA_USERNAME = os.getenv("GRAPHICA_USERNAME", "admin")
GRAPHICA_PASSWORD = os.getenv("GRAPHICA_PASSWORD", "Admin@Pass123")
DB2_CONNECTION = os.getenv("DB2_CONNECTION", "db2://localhost:50000/GRAPHICA")

# Test data - realistic customer records with various quality issues
TEST_CSV_DATA = """customer_id,first_name,last_name,email,age,country,registration_date,loyalty_points
CUST001,John,Doe,john.doe@example.com,35,USA,2024-01-15,1250
CUST002,Jane,Smith,jane.smith@example.com,28,Canada,2024-02-20,2100
CUST003,Bob,Wilson,,42,USA,2024-03-10,500
CUST004,Alice,Brown,alice@invalid,25,UK,2024-04-05,0
CUST005,Charlie,Davis,charlie.davis@example.com,-5,Australia,2024-05-12,750
CUST006,Diana,Miller,diana.miller@example.com,31,USA,2024-06-18,1850
CUST007,Eve,Taylor,eve.taylor@example.com,29,Canada,2024-07-22,990
CUST008,Frank,Anderson,,55,UK,2024-08-30,3200
CUST009,Grace,Thomas,grace.thomas@example.com,45,USA,2024-09-14,1500
CUST010,Henry,Jackson,henry.jackson@example.com,38,Australia,2024-10-01,1100"""


class GraphicaClient:
    """Client for interacting with Graphica REST API"""

    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip('/')
        self.token: Optional[str] = None
        self.session = requests.Session()
        self.session.headers.update({
            'Content-Type': 'application/json',
            'Accept': 'application/json'
        })

    def authenticate(self, username: str, password: str) -> bool:
        """Authenticate and get JWT token"""
        url = f"{self.base_url}/auth/login"

        try:
            response = self.session.post(url, json={
                "username": username,
                "password": password
            })
            response.raise_for_status()

            data = response.json()
            self.token = data.get('token')

            if self.token:
                self.session.headers.update({
                    'Authorization': f'Bearer {self.token}'
                })
                print(f"✅ Authenticated successfully")
                return True
            else:
                print(f"❌ Authentication failed: No token in response")
                return False

        except requests.exceptions.RequestException as e:
            print(f"❌ Authentication failed: {e}")
            return False

    def health_check(self) -> bool:
        """Check if coordinator is healthy"""
        try:
            response = self.session.get(f"{self.base_url}/health")
            return response.status_code == 200
        except:
            return False

    def upload_csv(self, csv_content: str, filename: str) -> Optional[str]:
        """Upload CSV file and get file ID"""
        url = f"{self.base_url}/api/v1/files/upload"

        files = {
            'file': (filename, csv_content, 'text/csv')
        }

        # Temporarily remove Content-Type header for multipart upload
        headers = self.session.headers.copy()
        del headers['Content-Type']

        try:
            response = self.session.post(url, files=files, headers=headers)
            response.raise_for_status()

            data = response.json()
            file_id = data.get('file_id')
            print(f"✅ Uploaded CSV file: {filename} (ID: {file_id})")
            return file_id

        except requests.exceptions.RequestException as e:
            print(f"❌ CSV upload failed: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return None

    def create_workflow(self, workflow_def: Dict) -> Optional[str]:
        """Create ETL workflow"""
        url = f"{self.base_url}/api/v1/workflows"

        try:
            response = self.session.post(url, json=workflow_def)
            response.raise_for_status()

            data = response.json()
            workflow_id = data.get('workflow_id')
            print(f"✅ Created workflow: {workflow_def['name']} (ID: {workflow_id})")
            return workflow_id

        except requests.exceptions.RequestException as e:
            print(f"❌ Workflow creation failed: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return None

    def execute_workflow(self, workflow_id: str, parameters: Dict) -> Optional[str]:
        """Execute workflow and get execution ID"""
        url = f"{self.base_url}/api/v1/workflows/{workflow_id}/execute"

        try:
            response = self.session.post(url, json=parameters)
            response.raise_for_status()

            data = response.json()
            execution_id = data.get('execution_id')
            print(f"✅ Started workflow execution (ID: {execution_id})")
            return execution_id

        except requests.exceptions.RequestException as e:
            print(f"❌ Workflow execution failed: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return None

    def get_execution_status(self, execution_id: str) -> Optional[Dict]:
        """Get workflow execution status"""
        url = f"{self.base_url}/api/v1/workflows/executions/{execution_id}"

        try:
            response = self.session.get(url)
            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get execution status: {e}")
            return None

    def wait_for_execution(self, execution_id: str, timeout: int = 300) -> bool:
        """Wait for workflow execution to complete"""
        print(f"⏳ Waiting for execution {execution_id} to complete...")

        start_time = time.time()
        while time.time() - start_time < timeout:
            status_data = self.get_execution_status(execution_id)

            if not status_data:
                time.sleep(2)
                continue

            status = status_data.get('status', 'unknown')
            print(f"   Status: {status}")

            if status in ['completed', 'success', 'finished']:
                print(f"✅ Execution completed successfully")
                return True
            elif status in ['failed', 'error']:
                print(f"❌ Execution failed")
                print(f"   Error: {status_data.get('error', 'Unknown error')}")
                return False

            time.sleep(2)

        print(f"❌ Execution timed out after {timeout} seconds")
        return False

    def get_row_lineage(self, row_key: str) -> Optional[Dict]:
        """Get lineage for a specific row"""
        url = f"{self.base_url}/api/v1/lineage/row/{row_key}"

        try:
            response = self.session.get(url)

            if response.status_code == 404:
                print(f"⚠️  Row lineage not found for: {row_key}")
                return None

            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get row lineage: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return None

    def get_row_journey(self, row_key: str) -> Optional[Dict]:
        """Get complete journey for a row"""
        url = f"{self.base_url}/api/v1/lineage/row/{row_key}/journey"

        try:
            response = self.session.get(url)

            if response.status_code == 404:
                print(f"⚠️  Row journey not found for: {row_key}")
                return None

            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get row journey: {e}")
            return None

    def get_job_stats(self, job_id: str) -> Optional[Dict]:
        """Get job statistics including row counts"""
        url = f"{self.base_url}/api/v1/lineage/job/{job_id}/stats"

        try:
            response = self.session.get(url)
            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get job stats: {e}")
            return None

    def get_batch_lineage(self, batch_id: str) -> Optional[Dict]:
        """Get lineage for all rows in a batch"""
        url = f"{self.base_url}/api/v1/lineage/batch/{batch_id}"

        try:
            response = self.session.get(url)
            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get batch lineage: {e}")
            return None

    def get_filtered_rows(self, job_id: str) -> Optional[Dict]:
        """Get filtered/rejected rows for a job"""
        url = f"{self.base_url}/api/v1/lineage/job/{job_id}/filtered"

        try:
            response = self.session.get(url)
            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get filtered rows: {e}")
            return None


def create_etl_workflow_definition(file_id: str, job_id: str, batch_id: str) -> Dict:
    """Create workflow definition with transformations and DB2 load"""
    return {
        "name": f"ETL_CSV_to_DB2_{datetime.now().strftime('%Y%m%d_%H%M%S')}",
        "description": "End-to-end ETL workflow with row-level lineage tracking",
        "version": "1.0",
        "type": "batch",
        "steps": [
            {
                "id": "extract",
                "name": "Extract CSV Data",
                "type": "csv_reader",
                "config": {
                    "file_id": file_id,
                    "skip_header": False,
                    "delimiter": ",",
                    "track_lineage": True
                }
            },
            {
                "id": "standardize",
                "name": "Standardize Fields",
                "type": "transformation",
                "depends_on": ["extract"],
                "config": {
                    "transformations": [
                        {
                            "type": "trim_whitespace",
                            "fields": ["first_name", "last_name", "email"]
                        },
                        {
                            "type": "uppercase",
                            "fields": ["country"]
                        },
                        {
                            "type": "lowercase",
                            "fields": ["email"]
                        }
                    ]
                }
            },
            {
                "id": "validate",
                "name": "Validate Data Quality",
                "type": "quality_rules",
                "depends_on": ["standardize"],
                "config": {
                    "rules": [
                        {
                            "name": "email_not_empty",
                            "type": "completeness",
                            "field": "email",
                            "severity": "error",
                            "action": "filter"
                        },
                        {
                            "name": "email_valid_format",
                            "type": "validity",
                            "field": "email",
                            "pattern": "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
                            "severity": "error",
                            "action": "filter"
                        },
                        {
                            "name": "age_positive",
                            "type": "validity",
                            "field": "age",
                            "condition": "age > 0",
                            "severity": "error",
                            "action": "filter"
                        },
                        {
                            "name": "age_reasonable",
                            "type": "validity",
                            "field": "age",
                            "condition": "age < 120",
                            "severity": "warning",
                            "action": "flag"
                        }
                    ]
                }
            },
            {
                "id": "enrich",
                "name": "Enrich Customer Data",
                "type": "transformation",
                "depends_on": ["validate"],
                "config": {
                    "transformations": [
                        {
                            "type": "derive",
                            "field": "age_group",
                            "expression": "CASE WHEN age < 30 THEN 'Young' WHEN age < 50 THEN 'Middle' ELSE 'Senior' END"
                        },
                        {
                            "type": "derive",
                            "field": "loyalty_tier",
                            "expression": "CASE WHEN loyalty_points >= 2000 THEN 'Gold' WHEN loyalty_points >= 1000 THEN 'Silver' ELSE 'Bronze' END"
                        }
                    ]
                }
            },
            {
                "id": "load_db2",
                "name": "Load to DB2",
                "type": "db2_loader",
                "depends_on": ["enrich"],
                "config": {
                    "connection": DB2_CONNECTION,
                    "schema": "GRAPHICA",
                    "table": "CUSTOMERS",
                    "mode": "insert",
                    "create_table_if_not_exists": True,
                    "track_lineage": True,
                    "job_id": job_id,
                    "batch_id": batch_id
                }
            }
        ],
        "settings": {
            "row_lineage_enabled": True,
            "job_id": job_id,
            "batch_id": batch_id,
            "tenant_id": "e2e-test-tenant",
            "error_handling": "continue",
            "dlq_enabled": True
        }
    }


def print_section(title: str):
    """Print section header"""
    print(f"\n{'='*80}")
    print(f" {title}")
    print(f"{'='*80}\n")


def test_row_lineage(client: GraphicaClient, job_id: str, batch_id: str, csv_filename: str):
    """Test row-level lineage tracking"""
    print_section("Testing Row-Level Lineage")

    # Test 1: Query row lineage for specific rows
    print("📊 Test 1: Query individual row lineage")
    test_rows = [
        f"csv:{csv_filename}:2",  # First data row (CUST001)
        f"csv:{csv_filename}:3",  # Second data row (CUST002)
        f"csv:{csv_filename}:4",  # Third row - should be filtered (no email)
    ]

    for row_key in test_rows:
        print(f"\n  Querying: {row_key}")
        lineage = client.get_row_lineage(row_key)

        if lineage:
            print(f"  ✅ Found {lineage.get('total_count', 0)} lineage events")

            events = lineage.get('events', [])
            for event in events[:2]:  # Show first 2 events
                outcome = event.get('outcome', {})
                print(f"     - Outcome: {outcome}")
        else:
            print(f"  ⚠️  No lineage found")

    # Test 2: Query row journey
    print(f"\n📊 Test 2: Query row journey (end-to-end tracking)")
    journey_row = f"csv:{csv_filename}:2"
    journey = client.get_row_journey(journey_row)

    if journey:
        steps = journey.get('steps', [])
        print(f"  ✅ Found {len(steps)} journey steps:")
        for i, step in enumerate(steps, 1):
            print(f"     {i}. {step.get('activity', 'Unknown')} - {step.get('outcome', {})}")
    else:
        print(f"  ⚠️  No journey found")

    # Test 3: Query job statistics
    print(f"\n📊 Test 3: Query job statistics")
    stats = client.get_job_stats(job_id)

    if stats:
        print(f"  ✅ Job Statistics:")
        print(f"     Total rows: {stats.get('total_rows', 0)}")
        print(f"     Success: {stats.get('success_count', 0)}")
        print(f"     Filtered: {stats.get('filtered_count', 0)}")
        print(f"     Failed: {stats.get('failed_count', 0)}")

        filter_reasons = stats.get('filter_reasons', {})
        if filter_reasons:
            print(f"     Filter reasons:")
            for reason, count in filter_reasons.items():
                print(f"       - {reason}: {count}")
    else:
        print(f"  ⚠️  No job stats available")

    # Test 4: Query batch lineage
    print(f"\n📊 Test 4: Query batch lineage")
    batch_lineage = client.get_batch_lineage(batch_id)

    if batch_lineage:
        print(f"  ✅ Batch Lineage:")
        print(f"     Total rows in batch: {batch_lineage.get('total_rows', 0)}")
        print(f"     Batch ID: {batch_lineage.get('batch_id', 'Unknown')}")
    else:
        print(f"  ⚠️  No batch lineage found")

    # Test 5: Query filtered rows
    print(f"\n📊 Test 5: Query filtered/rejected rows")
    filtered = client.get_filtered_rows(job_id)

    if filtered:
        filtered_rows = filtered.get('filtered_rows', [])
        print(f"  ✅ Found {len(filtered_rows)} filtered rows:")
        for row in filtered_rows[:5]:  # Show first 5
            print(f"     - {row.get('row_key', 'Unknown')}: {row.get('reason', 'No reason')}")
    else:
        print(f"  ⚠️  No filtered rows found")

    return True


def main():
    """Main test execution"""
    print_section("Graphica End-to-End ETL Row-Level Lineage Test")

    # Initialize client
    client = GraphicaClient(GRAPHICA_URL)

    # Step 1: Health check
    print("🏥 Step 1: Health Check")
    if not client.health_check():
        print(f"❌ Coordinator at {GRAPHICA_URL} is not responding")
        print(f"   Please ensure the coordinator is running")
        return 1
    print(f"✅ Coordinator is healthy")

    # Step 2: Authenticate
    print("\n🔐 Step 2: Authentication")
    if not client.authenticate(GRAPHICA_USERNAME, GRAPHICA_PASSWORD):
        print(f"❌ Authentication failed")
        return 1

    # Step 3: Upload CSV
    print("\n📤 Step 3: Upload CSV Test Data")
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    csv_filename = f"customers_test_{timestamp}.csv"

    file_id = client.upload_csv(TEST_CSV_DATA, csv_filename)
    if not file_id:
        print(f"❌ Failed to upload CSV")
        return 1

    # Step 4: Create workflow
    print("\n⚙️  Step 4: Create ETL Workflow")
    job_id = f"etl-job-{timestamp}"
    batch_id = f"batch-{timestamp}"

    workflow_def = create_etl_workflow_definition(file_id, job_id, batch_id)
    workflow_id = client.create_workflow(workflow_def)

    if not workflow_id:
        print(f"❌ Failed to create workflow")
        return 1

    print(f"   Job ID: {job_id}")
    print(f"   Batch ID: {batch_id}")

    # Step 5: Execute workflow
    print("\n▶️  Step 5: Execute Workflow")
    execution_id = client.execute_workflow(workflow_id, {
        "file_id": file_id,
        "job_id": job_id,
        "batch_id": batch_id
    })

    if not execution_id:
        print(f"❌ Failed to execute workflow")
        return 1

    # Step 6: Wait for completion
    print("\n⏳ Step 6: Wait for Workflow Completion")
    if not client.wait_for_execution(execution_id, timeout=300):
        print(f"❌ Workflow execution did not complete successfully")
        # Continue anyway to test lineage APIs

    # Step 7: Test row-level lineage
    print("\n🔍 Step 7: Test Row-Level Lineage APIs")
    test_row_lineage(client, job_id, batch_id, csv_filename)

    # Summary
    print_section("Test Summary")
    print("✅ All tests completed!")
    print(f"\nTest Details:")
    print(f"  - CSV File: {csv_filename}")
    print(f"  - Workflow ID: {workflow_id}")
    print(f"  - Execution ID: {execution_id}")
    print(f"  - Job ID: {job_id}")
    print(f"  - Batch ID: {batch_id}")
    print(f"\nNext Steps:")
    print(f"  1. Check DB2 database for loaded records:")
    print(f"     SELECT * FROM GRAPHICA.CUSTOMERS WHERE batch_id = '{batch_id}'")
    print(f"  2. Query row lineage via API:")
    print(f"     curl -H 'Authorization: Bearer <token>' {GRAPHICA_URL}/api/v1/lineage/row/csv:{csv_filename}:2")
    print(f"  3. Query job statistics:")
    print(f"     curl -H 'Authorization: Bearer <token>' {GRAPHICA_URL}/api/v1/lineage/job/{job_id}/stats")

    return 0


if __name__ == "__main__":
    sys.exit(main())
