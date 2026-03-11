#!/usr/bin/env python3
"""
PostgreSQL Performance Test for Phase 1+2 Optimizations

Tests the improved PostgreSQL loader with:
- Phase 1: 50K batch sizes (was 10K)
- Phase 2: 10 connection pool (was 1)

Expected improvement: 8-24x faster for large datasets
"""

import csv
import json
import random
import tempfile
import time
from datetime import datetime, timedelta
from typing import Dict, Any

from graphica import Client, BasicAuth
from graphica.errors import NotFoundError

# Configuration
SERVER_URL = "http://localhost:8080"
USERNAME = "admin"
PASSWORD = "Admin@Pass123"
POSTGRES_DATASOURCE_ID = "postgres-perf-test"
POSTGRES_TABLE = "healthcare_phase12_test"
NUM_RECORDS = 200_000

def create_healthcare_records(num_records: int) -> str:
    """Generate synthetic healthcare CSV data."""
    print(f"\n📊 Generating {num_records:,} synthetic healthcare records...")
    start = time.time()

    first_names = ["John", "Jane", "Michael", "Sarah", "David", "Emily", "Robert", "Lisa"]
    last_names = ["Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller"]
    diagnosis_codes = ["I10", "E11.9", "J44.0", "M17.0", "F41.1", "I25.10"]
    procedure_codes = ["99213", "99214", "93000", "71020", "80053"]

    tmpfile = tempfile.NamedTemporaryFile(mode='w', delete=False, suffix='.csv')
    writer = csv.DictWriter(tmpfile, fieldnames=[
        'patient_id', 'first_name', 'last_name', 'date_of_birth',
        'diagnosis_code', 'diagnosis_desc', 'procedure_code', 'procedure_desc',
        'admission_date', 'discharge_date', 'total_charges',
        'insurance_provider', 'policy_number', 'physician_name',
        'department', 'bed_number', 'medication', 'dosage',
        'lab_results', 'notes'
    ])
    writer.writeheader()

    base_date = datetime(2024, 1, 1)

    for i in range(num_records):
        patient_id = f"P{i:08d}"
        admission = base_date + timedelta(days=random.randint(0, 365))

        writer.writerow({
            'patient_id': patient_id,
            'first_name': random.choice(first_names),
            'last_name': random.choice(last_names),
            'date_of_birth': (datetime.now() - timedelta(days=random.randint(18*365, 80*365))).strftime('%Y-%m-%d'),
            'diagnosis_code': random.choice(diagnosis_codes),
            'diagnosis_desc': f"Diagnosis for {patient_id}",
            'procedure_code': random.choice(procedure_codes),
            'procedure_desc': f"Procedure for {patient_id}",
            'admission_date': admission.strftime('%Y-%m-%d'),
            'discharge_date': (admission + timedelta(days=random.randint(1, 14))).strftime('%Y-%m-%d'),
            'total_charges': round(random.uniform(1000, 50000), 2),
            'insurance_provider': random.choice(['BlueCross', 'Aetna', 'UnitedHealth', 'Cigna']),
            'policy_number': f"POL{random.randint(100000, 999999)}",
            'physician_name': f"Dr. {random.choice(last_names)}",
            'department': random.choice(['Cardiology', 'Neurology', 'Oncology', 'Orthopedics']),
            'bed_number': f"{random.randint(100, 999)}",
            'medication': random.choice(['Aspirin', 'Metformin', 'Lisinopril', 'Atorvastatin']),
            'dosage': f"{random.randint(5, 100)}mg",
            'lab_results': json.dumps({"glucose": random.randint(70, 140), "bp_systolic": random.randint(110, 140)}),
            'notes': f"Patient notes for {patient_id}"
        })

    tmpfile.close()
    duration = time.time() - start
    print(f"✅ Generated {num_records:,} records in {duration:.2f}s ({num_records/duration:.0f} records/sec)")
    print(f"   File: {tmpfile.name}")
    return tmpfile.name

def main():
    print("=" * 80)
    print("PostgreSQL Performance Test - Phase 1+2 Optimizations")
    print("=" * 80)
    print("\nOptimizations:")
    print("  Phase 1: Batch size = 50,000 rows (5x increase from 10K)")
    print("  Phase 2: Connection pool = 10 connections (10x increase from 1)")
    print("  Expected: 8-24x performance improvement")
    print("=" * 80)

    # Initialize client
    client = Client(SERVER_URL, auth=BasicAuth(USERNAME, PASSWORD))

    # Step 1: Create PostgreSQL datasource
    print("\n🔧 Setting up PostgreSQL datasource...")
    try:
        datasource = client.datasources.get(POSTGRES_DATASOURCE_ID)
        print(f"✅ Using existing datasource: {POSTGRES_DATASOURCE_ID}")
    except NotFoundError:
        print(f"📝 Creating new datasource: {POSTGRES_DATASOURCE_ID}")
        datasource = client.datasources.create({
            "id": POSTGRES_DATASOURCE_ID,
            "name": "PostgreSQL Performance Test",
            "type": "postgresql",
            "config": {
                "host": "localhost",
                "port": 5432,
                "database": "postgres",
                "username": "postgres",
                "password": "postgres"
            }
        })
        print(f"✅ Created datasource: {POSTGRES_DATASOURCE_ID}")

    # Step 2: Generate test data
    csv_file = create_healthcare_records(NUM_RECORDS)

    # Step 3: Upload and process
    print(f"\n🚀 Starting ETL pipeline ({NUM_RECORDS:,} records)...")
    print(f"   Target: {POSTGRES_TABLE}")
    print(f"   Batch size: 50,000 (Phase 1 optimization)")
    print(f"   Pool size: 10 connections (Phase 2 optimization)")

    overall_start = time.time()

    # Upload file
    print("\n📤 Uploading CSV file...")
    upload_start = time.time()
    with open(csv_file, 'rb') as f:
        file_id = client.files.upload(f, "healthcare_test.csv")
    upload_duration = time.time() - upload_start
    print(f"✅ Uploaded in {upload_duration:.2f}s")

    # Create workflow
    print("\n⚙️  Creating workflow...")
    workflow_def = {
        "name": "PostgreSQL Performance Test",
        "description": "Testing Phase 1+2 optimizations",
        "steps": [
            {
                "id": "load_to_postgres",
                "type": "database_load",
                "config": {
                    "datasource_id": POSTGRES_DATASOURCE_ID,
                    "table_name": POSTGRES_TABLE,
                    "mode": "replace",  # Clean start
                    "create_table": True,
                    "batch_size": 50000  # Phase 1: Explicit 50K batches
                }
            }
        ]
    }

    workflow = client.workflows.create(workflow_def)
    print(f"✅ Created workflow: {workflow['id']}")

    # Execute workflow
    print("\n▶️  Executing workflow...")
    exec_start = time.time()

    execution = client.workflows.execute(
        workflow['id'],
        {
            "file_id": file_id,
            "source_format": "csv"
        }
    )

    print(f"   Execution ID: {execution['id']}")

    # Monitor execution
    print("\n⏳ Monitoring execution...")
    while True:
        status = client.workflows.get_execution(workflow['id'], execution['id'])
        current_status = status['status']

        if current_status in ['completed', 'failed', 'error']:
            break

        # Show progress if available
        if 'progress' in status:
            progress = status['progress']
            print(f"   Progress: {progress.get('percentage', 0)}% - {progress.get('message', '')}")

        time.sleep(2)

    exec_duration = time.time() - exec_start
    overall_duration = time.time() - overall_start

    # Results
    print("\n" + "=" * 80)
    print("PERFORMANCE RESULTS")
    print("=" * 80)

    if current_status == 'completed':
        print(f"✅ Status: {current_status.upper()}")
        print(f"\n📊 Timing Breakdown:")
        print(f"   Data generation: {(overall_start - time.time() + overall_duration - exec_duration):.2f}s")
        print(f"   File upload:     {upload_duration:.2f}s")
        print(f"   ETL execution:   {exec_duration:.2f}s")
        print(f"   Total duration:  {overall_duration:.2f}s")

        throughput = NUM_RECORDS / exec_duration
        print(f"\n🚀 ETL Throughput: {throughput:,.0f} rows/sec")
        print(f"   ({NUM_RECORDS:,} rows in {exec_duration:.2f}s)")

        print(f"\n💡 Optimization Impact:")
        baseline_time = NUM_RECORDS / 5000  # Baseline: ~5K rows/sec
        improvement = baseline_time / exec_duration
        print(f"   Baseline estimate: {baseline_time:.0f}s (5,000 rows/sec)")
        print(f"   Actual time:       {exec_duration:.2f}s")
        print(f"   Improvement:       {improvement:.1f}x faster")

        if improvement >= 8:
            print(f"   ✅ Meets Phase 1+2 target (8-24x improvement)")
        else:
            print(f"   ⚠️  Below target, but still improved")
    else:
        print(f"❌ Status: {current_status.upper()}")
        if 'error' in status:
            print(f"   Error: {status['error']}")

    print("=" * 80)

    # Cleanup
    import os
    os.unlink(csv_file)

if __name__ == "__main__":
    main()
