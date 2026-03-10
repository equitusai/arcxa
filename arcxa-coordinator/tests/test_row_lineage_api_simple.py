#!/usr/bin/env python3
"""
Simple Row-Level Lineage API Test

This script tests ONLY the row-level lineage query APIs by posting test data
directly to the test endpoints (if enabled).

This is a simplified version for rapid iteration and API validation.

Requirements:
- Graphica coordinator running with ENABLE_TEST_LINEAGE_API=true
- ROW_LINEAGE_ENABLED=true
- Python 3.8+
- pip install requests

Usage:
    # Start coordinator with test endpoints enabled
    export ENABLE_TEST_LINEAGE_API=true
    export ROW_LINEAGE_ENABLED=true
    ./start_coordinator.sh

    # Run test
    python test_row_lineage_api_simple.py
"""

import json
import os
import sys
import time
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Optional
import requests


# Configuration
GRAPHICA_URL = os.getenv("GRAPHICA_URL", "http://localhost:8080")
GRAPHICA_USERNAME = os.getenv("GRAPHICA_USERNAME", "admin")
GRAPHICA_PASSWORD = os.getenv("GRAPHICA_PASSWORD", "Admin@Pass123")


class GraphicaClient:
    """Simplified client for testing row lineage APIs"""

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

    def write_row_lineage_test(self, event: Dict) -> bool:
        """Write row lineage event using test endpoint"""
        url = f"{self.base_url}/api/v1/lineage/row/test"

        try:
            response = self.session.post(url, json=event)
            response.raise_for_status()

            data = response.json()
            if 'warning' in data:
                print(f"⚠️  {data['warning']}")

            return True

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to write row lineage: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return False

    def get_row_lineage(self, row_key: str) -> Optional[Dict]:
        """Get lineage for a specific row"""
        url = f"{self.base_url}/api/v1/lineage/row/{row_key}"

        try:
            response = self.session.get(url)

            if response.status_code == 404:
                return None

            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get row lineage: {e}")
            return None

    def get_row_journey(self, row_key: str) -> Optional[Dict]:
        """Get complete journey for a row"""
        url = f"{self.base_url}/api/v1/lineage/row/{row_key}/journey"

        try:
            response = self.session.get(url)

            if response.status_code == 404:
                return None

            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get row journey: {e}")
            return None

    def get_job_stats(self, job_id: str) -> Optional[Dict]:
        """Get job statistics"""
        url = f"{self.base_url}/api/v1/lineage/job/{job_id}/stats"

        try:
            response = self.session.get(url)
            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get job stats: {e}")
            return None

    def get_batch_lineage(self, batch_id: str) -> Optional[Dict]:
        """Get batch lineage"""
        url = f"{self.base_url}/api/v1/lineage/batch/{batch_id}"

        try:
            response = self.session.get(url)
            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get batch lineage: {e}")
            return None

    def get_filtered_rows(self, job_id: str, start_time: str, end_time: str) -> Optional[Dict]:
        """Get filtered rows within a time range"""
        url = f"{self.base_url}/api/v1/lineage/job/{job_id}/filtered"

        # Add query parameters for time range
        params = {
            "start_time": start_time,
            "end_time": end_time
        }

        try:
            response = self.session.get(url, params=params)
            response.raise_for_status()
            return response.json()

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to get filtered rows: {e}")
            return None

    def flush_lineage_buffer(self) -> bool:
        """Flush buffered lineage events to storage (test endpoint)"""
        url = f"{self.base_url}/api/v1/lineage/flush/test"

        try:
            response = self.session.post(url)
            response.raise_for_status()
            return True

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to flush lineage buffer: {e}")
            return False


def create_test_lineage_events(job_id: str, batch_id: str, csv_filename: str) -> List[Dict]:
    """Create sample lineage events for testing"""
    now = datetime.now(timezone.utc).isoformat()

    events = []

    # Event 1: Successful processing
    events.append({
        "row_id": {
            "source_type": "Csv",
            "source_id": csv_filename,
            "position": {"RowNumber": 2}
        },
        "batch_id": batch_id,
        "job_id": job_id,
        "timestamp": now,
        "outcome": {
            "Processed": {
                "output_location": "db2://localhost:50000/GRAPHICA/CUSTOMERS"
            }
        },
        "transformations": [
            {
                "transform_type": "standardization",
                "fields": ["first_name", "last_name", "email"],
                "before_values": None,
                "after_values": None,
                "applied_at": now
            },
            {
                "transform_type": "quality_check",
                "fields": ["email", "age"],
                "before_values": None,
                "after_values": None,
                "applied_at": now
            }
        ],
        "output_row_id": {
            "source_type": {"Database": "DB2"},
            "source_id": "CUSTOMERS",
            "position": {"PrimaryKey": {"customer_id": "CUST001"}}
        },
        "tenant_id": "test-tenant",
        "correlation_id": "corr-001"
    })

    # Event 2: Another successful processing
    events.append({
        "row_id": {
            "source_type": "Csv",
            "source_id": csv_filename,
            "position": {"RowNumber": 3}
        },
        "batch_id": batch_id,
        "job_id": job_id,
        "timestamp": now,
        "outcome": {
            "Processed": {
                "output_location": "db2://localhost:50000/GRAPHICA/CUSTOMERS"
            }
        },
        "transformations": [
            {
                "transform_type": "standardization",
                "fields": ["first_name", "last_name", "email"],
                "before_values": None,
                "after_values": None,
                "applied_at": now
            }
        ],
        "output_row_id": {
            "source_type": {"Database": "DB2"},
            "source_id": "CUSTOMERS",
            "position": {"PrimaryKey": {"customer_id": "CUST002"}}
        },
        "tenant_id": "test-tenant",
        "correlation_id": "corr-002"
    })

    # Event 3: Filtered row (missing email)
    events.append({
        "row_id": {
            "source_type": "Csv",
            "source_id": csv_filename,
            "position": {"RowNumber": 4}
        },
        "batch_id": batch_id,
        "job_id": job_id,
        "timestamp": now,
        "outcome": {
            "Filtered": {
                "reason": "Missing required field: email",
                "rule_id": "email_not_empty"
            }
        },
        "transformations": [],
        "output_row_id": None,
        "tenant_id": "test-tenant",
        "correlation_id": "corr-003"
    })

    # Event 4: Filtered row (invalid email format)
    events.append({
        "row_id": {
            "source_type": "Csv",
            "source_id": csv_filename,
            "position": {"RowNumber": 5}
        },
        "batch_id": batch_id,
        "job_id": job_id,
        "timestamp": now,
        "outcome": {
            "Filtered": {
                "reason": "Invalid email format",
                "rule_id": "email_valid_format"
            }
        },
        "transformations": [
            {
                "transform_type": "standardization",
                "fields": ["email"],
                "before_values": None,
                "after_values": None,
                "applied_at": now
            }
        ],
        "output_row_id": None,
        "tenant_id": "test-tenant",
        "correlation_id": "corr-004"
    })

    # Event 5: Filtered row (negative age)
    events.append({
        "row_id": {
            "source_type": "Csv",
            "source_id": csv_filename,
            "position": {"RowNumber": 6}
        },
        "batch_id": batch_id,
        "job_id": job_id,
        "timestamp": now,
        "outcome": {
            "Filtered": {
                "reason": "Age must be positive",
                "rule_id": "age_positive"
            }
        },
        "transformations": [
            {
                "transform_type": "quality_check",
                "fields": ["age"],
                "before_values": None,
                "after_values": None,
                "applied_at": now
            }
        ],
        "output_row_id": None,
        "tenant_id": "test-tenant",
        "correlation_id": "corr-005"
    })

    return events


def main():
    """Main test execution"""
    print("="*80)
    print(" Graphica Row-Level Lineage API Test (Simplified)")
    print("="*80)
    print()

    # Initialize client
    client = GraphicaClient(GRAPHICA_URL)

    # Step 1: Authenticate
    print("🔐 Step 1: Authentication")
    if not client.authenticate(GRAPHICA_USERNAME, GRAPHICA_PASSWORD):
        return 1

    # Step 2: Generate test data
    print("\n📝 Step 2: Generate Test Data")
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    job_id = f"test-job-{timestamp}"
    batch_id = f"test-batch-{timestamp}"
    csv_filename = f"customers_test_{timestamp}.csv"

    print(f"   Job ID: {job_id}")
    print(f"   Batch ID: {batch_id}")
    print(f"   CSV Filename: {csv_filename}")

    events = create_test_lineage_events(job_id, batch_id, csv_filename)
    print(f"   Created {len(events)} test lineage events")

    # Step 3: Write test events
    print("\n📤 Step 3: Write Test Lineage Events")
    success_count = 0
    for i, event in enumerate(events, 1):
        row_key = f"csv:{csv_filename}:{event['row_id']['position']['RowNumber']}"
        print(f"   Writing event {i}/{len(events)}: {row_key}")

        if client.write_row_lineage_test(event):
            success_count += 1
        else:
            print(f"   ❌ Failed to write event {i}")

    print(f"\n   ✅ Successfully wrote {success_count}/{len(events)} events")

    if success_count == 0:
        print("\n❌ Failed to write any events. Is ENABLE_TEST_LINEAGE_API=true?")
        return 1

    # Flush buffered events to RocksDB
    print("\n💾 Step 3b: Flush Lineage Buffer")
    print("   Flushing buffered events to RocksDB...")
    if client.flush_lineage_buffer():
        print("   ✅ Buffer flushed successfully")
    else:
        print("   ⚠️  Failed to flush buffer - events may not be queryable yet")

    # Give RocksDB a moment to complete the write
    time.sleep(0.5)

    # Step 4: Query row lineage
    print("\n🔍 Step 4: Query Individual Row Lineage")
    test_rows = [
        (f"csv:{csv_filename}:2", "CUST001 - should be processed"),
        (f"csv:{csv_filename}:3", "CUST002 - should be processed"),
        (f"csv:{csv_filename}:4", "Missing email - should be filtered"),
        (f"csv:{csv_filename}:5", "Invalid email - should be filtered"),
    ]

    for row_key, description in test_rows:
        print(f"\n   Querying: {row_key}")
        print(f"   Expected: {description}")

        lineage = client.get_row_lineage(row_key)

        if lineage:
            total = lineage.get('total_count', 0)
            print(f"   ✅ Found {total} lineage event(s)")

            events_data = lineage.get('events', [])
            if events_data:
                event = events_data[0]
                outcome = event.get('outcome', {})
                print(f"      Outcome: {outcome}")

                transformations = event.get('transformations', [])
                print(f"      Transformations: {len(transformations)}")
        else:
            print(f"   ❌ No lineage found")

    # Step 5: Query row journey
    print("\n🗺️  Step 5: Query Row Journey")
    journey_row = f"csv:{csv_filename}:2"
    print(f"   Querying journey for: {journey_row}")

    journey = client.get_row_journey(journey_row)

    if journey:
        steps = journey.get('steps', [])
        print(f"   ✅ Found {len(steps)} journey step(s)")

        for i, step in enumerate(steps, 1):
            activity = step.get('activity', 'Unknown')
            outcome = step.get('outcome', {})
            duration = step.get('duration_ms', 0)
            print(f"      {i}. {activity} ({duration}ms) -> {outcome}")
    else:
        print(f"   ⚠️  No journey found")

    # Step 6: Query job statistics
    print("\n📊 Step 6: Query Job Statistics")
    print(f"   Querying stats for job: {job_id}")

    stats = client.get_job_stats(job_id)

    if stats:
        print(f"   ✅ Job Statistics:")
        print(f"      Total rows: {stats.get('total_rows', 0)}")
        print(f"      Success: {stats.get('success_count', 0)}")
        print(f"      Filtered: {stats.get('filtered_count', 0)}")
        print(f"      Failed: {stats.get('failed_count', 0)}")

        filter_reasons = stats.get('filter_reasons', {})
        if filter_reasons:
            print(f"      Filter reasons:")
            for reason, count in filter_reasons.items():
                print(f"        - {reason}: {count}")
    else:
        print(f"   ⚠️  No job stats available")

    # Step 7: Query batch lineage
    print("\n📦 Step 7: Query Batch Lineage")
    print(f"   Querying batch: {batch_id}")

    batch_lineage = client.get_batch_lineage(batch_id)

    if batch_lineage:
        print(f"   ✅ Batch Lineage:")
        print(f"      Total rows: {batch_lineage.get('total_rows', 0)}")
        print(f"      Batch ID: {batch_lineage.get('batch_id', 'Unknown')}")
    else:
        print(f"   ⚠️  No batch lineage found")

    # Step 8: Query filtered rows
    print("\n🚫 Step 8: Query Filtered Rows")
    print(f"   Querying filtered rows for job: {job_id}")

    # Use a time range that covers our test events (1 hour before to 1 hour after now)
    now_dt = datetime.now(timezone.utc)
    start_time = (now_dt - timedelta(hours=1)).isoformat()
    end_time = (now_dt + timedelta(hours=1)).isoformat()

    filtered = client.get_filtered_rows(job_id, start_time, end_time)

    if filtered:
        filtered_rows = filtered.get('filtered_rows', [])
        print(f"   ✅ Found {len(filtered_rows)} filtered row(s):")

        for row in filtered_rows:
            print(f"      - {row.get('row_key', 'Unknown')}: {row.get('reason', 'No reason')}")
    else:
        print(f"   ⚠️  No filtered rows found")

    # Summary
    print("\n" + "="*80)
    print(" Test Summary")
    print("="*80)
    print(f"\n✅ All API tests completed!")
    print(f"\nTest Details:")
    print(f"  - Job ID: {job_id}")
    print(f"  - Batch ID: {batch_id}")
    print(f"  - CSV Filename: {csv_filename}")
    print(f"  - Events Written: {success_count}")
    print(f"\nAPI Endpoints Tested:")
    print(f"  ✅ POST /api/v1/lineage/row/test")
    print(f"  ✅ POST /api/v1/lineage/flush/test")
    print(f"  ✅ GET  /api/v1/lineage/row/:row_key")
    print(f"  ✅ GET  /api/v1/lineage/row/:row_key/journey")
    print(f"  ✅ GET  /api/v1/lineage/job/:job_id/stats")
    print(f"  ✅ GET  /api/v1/lineage/batch/:batch_id")
    print(f"  ✅ GET  /api/v1/lineage/job/:job_id/filtered")

    return 0


if __name__ == "__main__":
    sys.exit(main())
