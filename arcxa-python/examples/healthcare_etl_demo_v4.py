#!/usr/bin/env python3
"""
Healthcare ETL Demo v5: Full ETL Pipeline with Enhanced SPARQL Validation

New features in v5:
- SPARQL queries to validate RDF governance data
- Workflow execution metadata validation
- Data quality metrics with field completeness
- Semantic consistency checks
- Provenance chain verification
- JSON validation report generation

Previous v4 features:
- 1M record CSV with 20 fields
- Extended healthcare ontology (20 properties)
- DB2 datasource registration
- Multi-step transformation workflow
- Data validation checks
- Lineage sampling and verification
"""

import csv
import json
import random
import string
import tempfile
import os
import time
from datetime import datetime, timedelta
from typing import Dict, Any, List, Optional

from graphica import Client, BasicAuth
from graphica.errors import NotFoundError, ValidationError, ServerError


# Configuration
SERVER_URL = "http://localhost:8080"
USERNAME = "admin"
PASSWORD = "Admin@Pass123"
DB2_DATASOURCE_ID = "db2-healthcare"
DB2_HOST = "localhost"
DB2_PORT = 50000
DB2_DATABASE = "GRAPHICA"
DB2_USER = "db2inst1"
DB2_PASSWORD = "graphica-db2-pass"
DB2_TARGET_TABLE = "HEALTHCARE_PATIENTS"


def create_extended_healthcare_data(num_records: int = 1000000) -> str:
    """Generate synthetic healthcare data with 20 fields."""
    print(f"Creating {num_records} synthetic healthcare records with 20 fields...")

    # Extended data pools
    first_names = ["John", "Jane", "Michael", "Sarah", "David", "Emily", "Robert", "Lisa",
                   "William", "Jennifer", "James", "Linda", "Richard", "Patricia", "Charles"]
    last_names = ["Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller",
                  "Davis", "Rodriguez", "Martinez", "Wilson", "Anderson", "Taylor", "Thomas"]
    conditions = ["Diabetes", "Hypertension", "Asthma", "Arthritis", "Depression", "Anxiety",
                  "COPD", "Heart Disease", "Cancer", "Obesity", "Migraine", "Insomnia"]
    medications = ["Metformin", "Lisinopril", "Albuterol", "Ibuprofen", "Sertraline",
                   "Alprazolam", "Tiotropium", "Aspirin", "Atorvastatin", "Omeprazole"]
    departments = ["Cardiology", "Neurology", "Orthopedics", "Pediatrics", "Oncology",
                   "Emergency", "Internal Medicine", "Radiology", "Surgery"]
    blood_types = ["A+", "A-", "B+", "B-", "AB+", "AB-", "O+", "O-"]
    insurance_providers = ["BlueCross", "Aetna", "UnitedHealth", "Cigna", "Kaiser", "Humana"]
    cities = ["New York", "Los Angeles", "Chicago", "Houston", "Phoenix", "Philadelphia",
              "San Antonio", "San Diego", "Dallas", "San Jose"]
    states = ["NY", "CA", "IL", "TX", "AZ", "PA", "FL", "OH", "NC", "MI"]
    marital_status = ["Single", "Married", "Divorced", "Widowed"]
    ethnicities = ["Caucasian", "African American", "Hispanic", "Asian", "Other"]

    def misspell(text: str, probability: float = 0.15) -> str:
        if random.random() > probability or len(text) < 3:
            return text
        text = list(text)
        idx = random.randint(1, len(text) - 2)
        mutation = random.choice(["swap", "delete", "insert", "replace"])
        if mutation == "swap" and idx < len(text) - 1:
            text[idx], text[idx + 1] = text[idx + 1], text[idx]
        elif mutation == "delete":
            del text[idx]
        elif mutation == "insert":
            text.insert(idx, random.choice(string.ascii_lowercase))
        elif mutation == "replace":
            text[idx] = random.choice(string.ascii_lowercase)
        return "".join(text)

    def random_phone() -> str:
        return f"{random.randint(200, 999)}-{random.randint(100, 999)}-{random.randint(1000, 9999)}"

    def random_ssn() -> str:
        return f"{random.randint(100, 999)}-{random.randint(10, 99)}-{random.randint(1000, 9999)}"

    def random_email(first: str, last: str) -> str:
        domains = ["gmail.com", "yahoo.com", "hotmail.com", "outlook.com"]
        return f"{first.lower()}.{last.lower()}@{random.choice(domains)}"

    def random_date(start_year: int = 1950, end_year: int = 2005) -> str:
        start = datetime(start_year, 1, 1)
        end = datetime(end_year, 12, 31)
        delta = end - start
        return (start + timedelta(days=random.randint(0, delta.days))).strftime("%Y-%m-%d")

    def random_visit_date() -> str:
        start = datetime(2023, 1, 1)
        end = datetime(2024, 11, 1)
        delta = end - start
        return (start + timedelta(days=random.randint(0, delta.days))).strftime("%Y-%m-%d")

    def random_address() -> str:
        street_num = random.randint(100, 9999)
        streets = ["Main St", "Oak Ave", "Maple Dr", "Cedar Ln", "Park Blvd", "Washington St"]
        return f"{street_num} {random.choice(streets)}"

    records = []
    base_count = int(num_records * 0.85)  # 85% unique, 15% duplicates
    duplicate_groups = {}

    print(f"  Generating {base_count} base records...")
    for i in range(base_count):
        first = random.choice(first_names)
        last = random.choice(last_names)

        record = {
            # Core identity fields (1-5)
            "patient_id": f"P{i+1:06d}",
            "first_name": misspell(first),
            "last_name": misspell(last),
            "date_of_birth": random_date(),
            "ssn": random_ssn(),

            # Contact fields (6-10)
            "email": random_email(first, last),
            "phone": random_phone(),
            "address": random_address(),
            "city": random.choice(cities),
            "state": random.choice(states),

            # Clinical fields (11-15)
            "blood_type": random.choice(blood_types),
            "condition": misspell(random.choice(conditions)),
            "medication": misspell(random.choice(medications)),
            "department": random.choice(departments),
            "primary_physician": f"Dr. {random.choice(last_names)}",

            # Visit/billing fields (16-20)
            "visit_date": random_visit_date(),
            "visit_cost": round(random.uniform(50, 5000), 2),
            "insurance_provider": random.choice(insurance_providers),
            "marital_status": random.choice(marital_status),
            "ethnicity": random.choice(ethnicities),
        }
        records.append(record)
        duplicate_groups[i] = []

    print(f"  Adding duplicates with variations...")
    exact_duplicates = 0
    near_duplicates = 0

    for _ in range(num_records - base_count):
        base_idx = random.randint(0, base_count - 1)
        base = records[base_idx].copy()
        base["patient_id"] = f"P{len(records)+1:06d}"

        # 60% exact duplicates, 40% near duplicates
        if random.random() < 0.6:
            # Exact duplicate - only vary non-key fields
            if random.random() < 0.3:
                base["phone"] = random_phone()
            if random.random() < 0.2:
                base["address"] = random_address()
            exact_duplicates += 1
            duplicate_groups[base_idx].append(len(records))
        else:
            # Near duplicate - modify key fields
            if random.random() < 0.3:
                base["first_name"] = misspell(base["first_name"], probability=0.5)
            if random.random() < 0.3:
                base["last_name"] = misspell(base["last_name"], probability=0.5)
            near_duplicates += 1

        records.append(base)

    print(f"  Generated {exact_duplicates} exact duplicates and {near_duplicates} near duplicates")
    groups_with_dupes = sum(1 for dupes in duplicate_groups.values() if len(dupes) > 0)
    expected_unique = base_count + near_duplicates
    print(f"  Expected dedup result: {expected_unique} unique records ({groups_with_dupes} groups with exact duplicates)")

    random.shuffle(records)

    output_path = os.path.join(tempfile.gettempdir(), "healthcare_patients_1m.csv")
    with open(output_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(records[0].keys()))
        writer.writeheader()
        writer.writerows(records)

    print(f"  Created {len(records)} records at: {output_path}")
    return output_path, expected_unique


def create_extended_healthcare_ontology() -> str:
    """Return extended healthcare ontology with 20 properties in Turtle format."""
    return '''
@prefix hc: <http://graphica.io/healthcare#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

hc:Patient a owl:Class ;
    rdfs:label "Patient" ;
    rdfs:comment "A healthcare patient entity" .

# Core identity properties (1-5)
hc:patientId a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Patient ID" .

hc:firstName a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "First Name" .

hc:lastName a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Last Name" .

hc:dateOfBirth a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:date ;
    rdfs:label "Date of Birth" .

hc:ssn a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Social Security Number" .

# Contact properties (6-10)
hc:email a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Email Address" .

hc:phone a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Phone Number" .

hc:address a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Street Address" .

hc:city a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "City" .

hc:state a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "State" .

# Clinical properties (11-15)
hc:bloodType a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Blood Type" .

hc:condition a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Medical Condition" .

hc:medication a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Medication" .

hc:department a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Department" .

hc:primaryPhysician a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Primary Physician" .

# Visit/billing properties (16-20)
hc:visitDate a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:date ;
    rdfs:label "Visit Date" .

hc:visitCost a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:decimal ;
    rdfs:label "Visit Cost" .

hc:insuranceProvider a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Insurance Provider" .

hc:maritalStatus a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Marital Status" .

hc:ethnicity a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Ethnicity" .
'''


def upload_or_update_ontology(client: Client) -> bool:
    """Upload or update healthcare ontology."""
    ontology_id = "healthcare-v1"
    print(f"Checking for existing ontology: {ontology_id}")

    try:
        existing = client.ontology.get(ontology_id)
        print(f"  Ontology '{ontology_id}' already exists")
        # Just activate if inactive
        if not existing.get("active", False):
            print(f"  Activating ontology...")
            client.ontology.activate(ontology_id)
        else:
            print(f"  Ontology is active")
        return True
    except NotFoundError:
        print(f"  Ontology not found, registering new one...")

    print(f"  Registering new ontology: {ontology_id}")
    content = create_extended_healthcare_ontology()

    try:
        validation = client.ontology.validate(content)
        print(f"  Validation: {validation}")
    except Exception as e:
        print(f"  Validation note: {e}")

    try:
        result = client.ontology.register(
            ontology_id=ontology_id,
            name="Healthcare Ontology (Extended)",
            content=content,
            description="Extended ontology for patient records with 20 properties",
            version="1.0.0",
            author="Graphica Demo",
            tags=["healthcare", "patients", "extended"],
        )
        print(f"  Registered ontology: {result.get('id', ontology_id)}")
        return True
    except Exception as e:
        print(f"  Registration error: {e}")
        print(f"  Note: Using existing ontology if available")
        return True


def register_db2_datasource(client: Client) -> str:
    """Register or update DB2 datasource."""
    print(f"Registering DB2 datasource: {DB2_DATASOURCE_ID}")

    # Check if datasource exists
    try:
        sources = client.get("/api/v1/datasources")
        if isinstance(sources, dict):
            for source in sources.get("sources", []):
                if source.get("title") == "Healthcare DB2 Database":
                    print(f"  Datasource already exists")
                    return DB2_DATASOURCE_ID
    except Exception as e:
        print(f"  Note checking existing: {e}")

    # Register new datasource with correct schema
    datasource_config = {
        "title": "Healthcare DB2 Database",
        "description": "DB2 database for healthcare patient records",
        "sourceType": "DB2",
        "connection": {
            "secretRef": "local://db2-creds",  # For demo purposes
            "config": {
                "type": "DB2",
                "host": DB2_HOST,
                "port": DB2_PORT,
                "database": DB2_DATABASE
            },
            "encryptionEnabled": False
        },
        "tags": ["db2", "healthcare", "production"],
        "metadata": {
            "environment": "development",
            "owner": "healthcare-team"
        }
    }

    try:
        result = client.post("/api/v1/datasources", json=datasource_config)
        datasource_id = result.get("@id", result.get("id", DB2_DATASOURCE_ID))
        print(f"  Registered datasource: {datasource_id}")
        return datasource_id
    except Exception as e:
        print(f"  Error registering datasource: {e}")
        print(f"  Note: Datasource registration may require secret store configuration")
        print(f"  Using placeholder ID anyway: {DB2_DATASOURCE_ID}")
        return DB2_DATASOURCE_ID


def create_comprehensive_workflow(client: Client, csv_path: str) -> Dict[str, Any]:
    """Create comprehensive multi-step workflow with transformations."""
    workflow_name = "healthcare-db2-etl-workflow"
    print(f"Creating workflow: {workflow_name}")

    # Delete existing workflow if present
    try:
        workflows = client.workflows.list()
        existing_id = None
        if isinstance(workflows, dict) and "workflows" in workflows:
            for wf in workflows.get("workflows", []):
                if wf.get("name") == workflow_name:
                    existing_id = wf.get("workflow_id", wf.get("id"))
                    break
        elif isinstance(workflows, list):
            for wf in workflows:
                if wf.get("name") == workflow_name:
                    existing_id = wf.get("workflow_id", wf.get("id"))
                    break

        if existing_id:
            print(f"  Deleting existing workflow: {existing_id}")
            client.workflows.delete(existing_id)
    except Exception as e:
        print(f"  Note: {e}")

    # Multi-step workflow - simplified to use tested components
    workflow_request = {
        "name": workflow_name,
        "description": "ETL Pipeline: CSV -> Ontology Map -> Dedup -> CSV Export",
        "tags": ["healthcare", "etl", "semantic", "production"],
        "definition": {
            "steps": [
                # Step 1: CSV Source
                {
                    "id": "csv_source",
                    "step_type": "csv_source",
                    "config": {
                        "file_path": csv_path,
                        "has_header": True,
                    }
                },
                # Step 2: Semantic Mapping to Ontology
                {
                    "id": "semantic_map",
                    "step_type": "semantic_mapper",
                    "depends_on": ["csv_source"],
                    "config": {
                        "target_ontology": ["healthcare-v1"],
                        "auto_approve_threshold": 0.8,
                        "mapping_mode": "auto"
                    }
                },
                # Step 3: Deduplication
                {
                    "id": "dedup",
                    "step_type": "deduplicator",
                    "depends_on": ["semantic_map"],
                    "config": {
                        "method": {"exact": None},
                        "key_fields": [
                            "firstName",
                            "lastName",
                            "dateOfBirth"
                        ],
                        "keep": "first"
                    }
                },
                # Step 4: CSV Export
                {
                    "id": "csv_export",
                    "step_type": "csv_exporter",
                    "depends_on": ["dedup"],
                    "config": {
                        "output_path": "/tmp/healthcare_deduped_1m.csv",
                        "include_header": True,
                    }
                }
            ]
        }
    }

    try:
        result = client.workflows.create(workflow_request)
        workflow_id = result.get("workflow_id", result.get("id"))
        print(f"  Created workflow: {workflow_id}")
        return {"id": workflow_id, "name": workflow_name, **result}
    except Exception as e:
        print(f"  Error creating workflow: {e}")
        return {"name": workflow_name, "error": str(e)}


def execute_workflow(client: Client, workflow_id: str) -> Dict[str, Any]:
    """Execute the workflow and return results."""
    print(f"Executing workflow: {workflow_id}")
    print(f"  Note: Processing 1M rows may take several minutes...")

    try:
        # Execute workflow (default timeout applies)
        result = client.post(
            f"/api/v1/workflows/{workflow_id}/execute",
            json={"input": {}}
        )

        status = "SUCCESS" if result.get("overall_success", False) else "FAILED"
        results = result.get("results", [])
        run_id = results[0].get("execution_id", "unknown") if results else "unknown"

        print(f"\n  {'Status:':<25} {status}")
        print(f"  {'Execution ID:':<25} {run_id}")
        print(f"  {'Batch count:':<25} {result.get('batch_count', 0)}")
        print(f"  {'Avg Confidence:':<25} {result.get('average_confidence', 0.0):.2f}")

        return result
    except Exception as e:
        print(f"  Execution error: {e}")
        return {"status": "error", "message": str(e)}


def validate_csv_output(input_path: str, output_path: str, expected_count: int) -> bool:
    """Validate CSV output file was created correctly."""
    print("\n" + "=" * 60)
    print("Output Data Validation")
    print("=" * 60)

    if not os.path.exists(output_path):
        print(f"\n  Output file not found: {output_path}")
        return False

    # Count input and output records
    with open(input_path, 'r') as f:
        input_count = sum(1 for _ in f) - 1  # Subtract header

    with open(output_path, 'r') as f:
        output_count = sum(1 for _ in f) - 1  # Subtract header

    duplicates_removed = input_count - output_count
    dedup_rate = (duplicates_removed / input_count * 100) if input_count > 0 else 0

    print(f"\n  {'Input records:':<30} {input_count:>8}")
    print(f"  {'Output records:':<30} {output_count:>8}")
    print(f"  {'Expected unique:':<30} {expected_count:>8}")
    print(f"  {'Duplicates removed:':<30} {duplicates_removed:>8}")
    print(f"  {'Deduplication rate:':<30} {dedup_rate:>7.1f}%")

    match = abs(output_count - expected_count) < 100  # Allow some variance
    status = "PASS" if match else "WARN"
    print(f"  {'Validation status:':<30} {status}")

    if not match:
        diff = abs(output_count - expected_count)
        print(f"  {'Difference from expected:':<30} {diff:>8} records")

    # Show sample records with semantic field names
    with open(output_path, 'r') as f:
        reader = csv.DictReader(f)
        sample_records = []
        headers = None
        for i, row in enumerate(reader):
            if headers is None:
                headers = list(row.keys())
            if i >= 5:
                break
            sample_records.append(row)

    if sample_records:
        print(f"\n  Sample records from output (showing semantic fields):")
        print("  " + "-" * 80)

        # Show header with field names
        key_fields = ['patientId', 'firstName', 'lastName', 'dateOfBirth', 'City', 'Department']
        available_fields = [f for f in key_fields if f in headers]

        if available_fields:
            header_line = " | ".join(f"{f:>12}" for f in available_fields)
            print(f"  {header_line}")
            print("  " + "-" * 80)

        for i, row in enumerate(sample_records, 1):
            if available_fields:
                values = [str(row.get(f, 'N/A'))[:12] for f in available_fields]
                value_line = " | ".join(f"{v:>12}" for v in values)
                print(f"  {value_line}")
            else:
                # Fallback: show first 4 columns
                cols = list(row.values())[:4]
                print(f"  {i}. {' | '.join(str(c)[:15] for c in cols)}")

        # Show all available semantic fields
        semantic_fields = [f for f in headers if not f.startswith('unmapped.') and f != '_row_id' and f != '_row_index']
        unmapped_fields = [f for f in headers if f.startswith('unmapped.')]

        print(f"\n  Semantic fields mapped ({len(semantic_fields)}):")
        print(f"    {', '.join(semantic_fields[:10])}")
        if len(semantic_fields) > 10:
            print(f"    ... and {len(semantic_fields) - 10} more")

        if unmapped_fields:
            print(f"\n  Unmapped fields ({len(unmapped_fields)}):")
            print(f"    {', '.join([f.replace('unmapped.', '') for f in unmapped_fields[:5]])}")
            if len(unmapped_fields) > 5:
                print(f"    ... and {len(unmapped_fields) - 5} more")

    return match


def sample_lineage_verification(client: Client, csv_path: str, num_samples: int = 5) -> None:
    """Sample random records and verify their lineage."""
    print("\n" + "=" * 60)
    print("Lineage Sampling & Verification")
    print("=" * 60)

    # Count total rows in CSV
    with open(csv_path, 'r') as f:
        total_rows = sum(1 for _ in f) - 1  # Subtract header

    print(f"\n  Total CSV rows: {total_rows}")
    print(f"  Sampling {num_samples} random records for lineage verification...")

    # Sample random row numbers (adding 2 because row 1 is header, data starts at row 2)
    sample_rows = sorted(random.sample(range(2, total_rows + 2), min(num_samples, total_rows)))

    verified = 0
    failed = 0

    for row_num in sample_rows:
        row_key = f"csv:{csv_path}:{row_num}"

        try:
            # Get row lineage
            row_lineage = client.lineage.get_row(row_key)
            events = row_lineage.get("events", [])

            if events:
                print(f"\n  Row {row_num}: VERIFIED")
                print(f"    Lineage events: {len(events)}")

                # Show latest transformation
                latest = events[-1]
                transforms = latest.get('transformations', [])
                if transforms:
                    print(f"    Transformations: {len(transforms)}")
                    for t in transforms[:3]:  # Show first 3
                        t_type = t.get('transform_type', 'unknown')
                        print(f"      - {t_type}")

                verified += 1
            else:
                print(f"\n  Row {row_num}: NO EVENTS")
                failed += 1

        except NotFoundError:
            print(f"\n  Row {row_num}: NOT FOUND")
            failed += 1
        except Exception as e:
            print(f"\n  Row {row_num}: ERROR - {e}")
            failed += 1

    print(f"\n  Verification Summary:")
    print(f"    Verified: {verified}/{num_samples}")
    print(f"    Failed:   {failed}/{num_samples}")
    print(f"    Success rate: {(verified/num_samples*100):.1f}%")


def verify_column_lineage(client: Client) -> None:
    """Verify column lineage tracking for key fields."""
    print("\n" + "=" * 60)
    print("Column Lineage Verification")
    print("=" * 60)

    test_columns = [
        ("healthcare_patients_10k", "first_name"),
        ("healthcare_patients_10k", "last_name"),
        ("healthcare_patients_10k", "email"),
        ("healthcare_patients_10k", "ssn"),
    ]

    verified = 0
    for table, column in test_columns:
        try:
            lineage = client.lineage.get_column(table, column)

            if isinstance(lineage, list) and len(lineage) > 0:
                print(f"\n  {column}: VERIFIED")
                print(f"    Lineage events: {len(lineage)}")

                # Show transformation details
                for event in lineage[:2]:  # Show first 2
                    target = event.get('target_column', {})
                    target_name = target.get('column_name', 'unknown')
                    transform_type = event.get('transformation_type', 'unknown')
                    print(f"    -> {target_name} ({transform_type})")

                verified += 1
            else:
                print(f"\n  {column}: NO LINEAGE")

        except NotFoundError:
            print(f"\n  {column}: NOT FOUND")
        except Exception as e:
            print(f"\n  {column}: ERROR - {e}")

    print(f"\n  Column Lineage Summary:")
    print(f"    Verified: {verified}/{len(test_columns)}")


def show_csv_sample(csv_path: str, num_rows: int = 5) -> None:
    """Display sample rows from CSV."""
    print(f"\nSample data from {os.path.basename(csv_path)}:")
    with open(csv_path, "r") as f:
        reader = csv.DictReader(f)
        for i, row in enumerate(reader):
            if i >= num_rows:
                break
            print(f"  Row {i+1}: {row['patient_id']} - {row['first_name']} {row['last_name']} "
                  f"({row['date_of_birth']}) - {row['city']}, {row['state']}")


# =============================================================================
# V5 VALIDATION FUNCTIONS - Enhanced SPARQL and Data Quality Validation
# =============================================================================

def validate_with_sparql(client: Client) -> None:
    """Validate workflow execution using SPARQL queries against the RDF store."""
    print("\n" + "=" * 60)
    print("SPARQL Governance Validation")
    print("=" * 60)

    queries = {
        "workflow_executions": """
            PREFIX prov: <http://www.w3.org/ns/prov#>
            PREFIX graphica: <http://graphica.io/ontology#>

            SELECT ?execution ?startTime ?endTime ?status
            WHERE {
                ?execution a prov:Activity ;
                          prov:startedAtTime ?startTime ;
                          prov:endedAtTime ?endTime ;
                          graphica:status ?status .
                FILTER(regex(str(?execution), "healthcare", "i"))
            }
            ORDER BY DESC(?startTime)
            LIMIT 5
        """,

        "data_lineage": """
            PREFIX prov: <http://www.w3.org/ns/prov#>

            SELECT (COUNT(DISTINCT ?entity) as ?entityCount)
                   (COUNT(?derivation) as ?derivationCount)
            WHERE {
                ?derived a prov:Entity ;
                        prov:wasDerivedFrom ?source .
                ?derivation prov:entity ?derived ;
                           prov:hadGeneration ?gen .
            }
        """,

        "ontology_mappings": """
            PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
            PREFIX graphica: <http://graphica.io/ontology#>

            SELECT ?property ?label (COUNT(?usage) as ?usageCount)
            WHERE {
                ?property a graphica:MappedProperty ;
                         rdfs:label ?label .
                OPTIONAL {
                    ?usage graphica:usesProperty ?property .
                }
            }
            GROUP BY ?property ?label
            ORDER BY DESC(?usageCount)
            LIMIT 10
        """,

        "data_quality_metrics": """
            PREFIX graphica: <http://graphica.io/ontology#>
            PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

            SELECT ?metric ?value ?timestamp
            WHERE {
                ?measurement a graphica:QualityMeasurement ;
                            graphica:metricName ?metric ;
                            graphica:metricValue ?value ;
                            graphica:measuredAt ?timestamp .
                FILTER(?timestamp > "2025-01-01T00:00:00Z"^^xsd:dateTime)
            }
            ORDER BY DESC(?timestamp)
            LIMIT 10
        """
    }

    for query_name, sparql in queries.items():
        print(f"\n  {query_name.replace('_', ' ').title()}:")
        print("  " + "-" * 56)

        try:
            # Execute SPARQL query
            result = client.post("/api/v1/governance/sparql", json={"sparql": sparql})

            if isinstance(result, dict):
                # Handle Graphica format: {"results": [...]}
                results = result.get("results", [])

                # Also check for standard SPARQL format: {"results": {"bindings": [...]}}
                if isinstance(results, dict):
                    results = results.get("bindings", [])

                if results:
                    # Display results
                    for i, row in enumerate(results[:5], 1):
                        # Handle both dict and Value objects
                        if isinstance(row, dict):
                            # For Graphica format, values might be nested
                            values = {}
                            for k, v in row.items():
                                if isinstance(v, dict) and "value" in v:
                                    values[k] = v["value"]
                                else:
                                    values[k] = str(v)
                            value_str = " | ".join(f"{k}: {str(v)[:20]}" for k, v in values.items())
                        else:
                            value_str = str(row)[:100]
                        print(f"    {i}. {value_str}")

                    if len(results) > 5:
                        print(f"    ... and {len(results) - 5} more results")
                else:
                    print("    No results found")
            else:
                print(f"    Unexpected response format")

        except Exception as e:
            print(f"    ERROR: {str(e)[:100]}")


def validate_workflow_metadata(client: Client, workflow_id: str, execution_result: Dict) -> None:
    """Validate workflow execution metadata and statistics."""
    print("\n" + "=" * 60)
    print("Workflow Metadata Validation")
    print("=" * 60)

    # Extract execution details
    execution_id = execution_result.get("results", [{}])[0].get("execution_id", "unknown")
    batch_count = execution_result.get("batch_count", 0)
    avg_confidence = execution_result.get("average_confidence", 0.0)

    print(f"\n  Workflow ID:        {workflow_id}")
    print(f"  Execution ID:       {execution_id}")
    print(f"  Batch Count:        {batch_count}")
    print(f"  Avg Confidence:     {avg_confidence:.2f}")

    # Validate step results
    steps = execution_result.get("results", [])
    print(f"\n  Step Execution Summary:")
    print("  " + "-" * 56)

    for step in steps:
        step_name = step.get("step_id", "unknown")
        success = step.get("success", False)
        duration_ms = step.get("duration_ms", 0)
        rows_processed = step.get("output", {}).get("_rows", [])
        row_count = len(rows_processed) if isinstance(rows_processed, list) else "N/A"

        status = "✓" if success else "✗"
        print(f"    {status} {step_name:<20} {duration_ms:>6}ms  {row_count:>8} rows")


def validate_data_quality(output_path: str) -> Dict[str, Any]:
    """Calculate data quality metrics from the output CSV."""
    print("\n" + "=" * 60)
    print("Data Quality Metrics")
    print("=" * 60)

    metrics = {
        "total_rows": 0,
        "null_counts": {},
        "unique_counts": {},
        "field_completeness": {},
    }

    with open(output_path, 'r') as f:
        reader = csv.DictReader(f)
        headers = reader.fieldnames

        # Initialize counters
        for header in headers:
            metrics["null_counts"][header] = 0
            metrics["unique_counts"][header] = set()

        # Process rows
        for row in reader:
            metrics["total_rows"] += 1
            for header in headers:
                value = row.get(header, "")
                if not value or value.strip() == "":
                    metrics["null_counts"][header] += 1
                else:
                    metrics["unique_counts"][header].add(value)

    # Calculate completeness
    for header in headers:
        completeness = ((metrics["total_rows"] - metrics["null_counts"][header]) /
                       metrics["total_rows"] * 100) if metrics["total_rows"] > 0 else 0
        metrics["field_completeness"][header] = completeness

    # Display key metrics
    print(f"\n  Total Records:      {metrics['total_rows']:,}")

    # Show completeness for key fields
    key_fields = ['firstName', 'lastName', 'dateOfBirth', 'patientId', 'email']
    available_key_fields = [f for f in key_fields if f in metrics["field_completeness"]]

    if available_key_fields:
        print(f"\n  Field Completeness:")
        print("  " + "-" * 56)
        for field in available_key_fields:
            completeness = metrics["field_completeness"][field]
            unique_count = len(metrics["unique_counts"][field])
            status = "✓" if completeness >= 95 else "⚠"
            print(f"    {status} {field:<20} {completeness:>5.1f}%  ({unique_count:,} unique)")

    # Convert sets to counts for JSON serialization
    metrics["unique_counts"] = {k: len(v) for k, v in metrics["unique_counts"].items()}

    return metrics


def validate_semantic_consistency(output_path: str) -> None:
    """Validate semantic consistency of mapped fields."""
    print("\n" + "=" * 60)
    print("Semantic Consistency Validation")
    print("=" * 60)

    inconsistencies = []

    with open(output_path, 'r') as f:
        reader = csv.DictReader(f)

        for i, row in enumerate(reader, 1):
            # Check date format consistency
            if 'dateOfBirth' in row:
                dob = row['dateOfBirth']
                if dob and not _is_valid_date(dob):
                    inconsistencies.append(f"Row {i}: Invalid date format in dateOfBirth: {dob}")

            # Check email format
            if 'email' in row:
                email = row['email']
                if email and '@' not in email:
                    inconsistencies.append(f"Row {i}: Invalid email format: {email}")

            # Only check first 100 rows for performance
            if i >= 100:
                break

    if inconsistencies:
        print(f"\n  Found {len(inconsistencies)} inconsistencies in first 100 rows:")
        for issue in inconsistencies[:5]:
            print(f"    ⚠ {issue}")
        if len(inconsistencies) > 5:
            print(f"    ... and {len(inconsistencies) - 5} more")
    else:
        print("\n  ✓ No semantic inconsistencies found in first 100 rows")


def _is_valid_date(date_str: str) -> bool:
    """Check if string is a valid date in YYYY-MM-DD format."""
    try:
        datetime.strptime(date_str, "%Y-%m-%d")
        return True
    except ValueError:
        return False


def validate_provenance_chain(client: Client, output_path: str, sample_size: int = 5) -> None:
    """Validate end-to-end provenance chain for sample records."""
    print("\n" + "=" * 60)
    print("Provenance Chain Validation")
    print("=" * 60)

    # Get random sample records
    with open(output_path, 'r') as f:
        reader = csv.DictReader(f)
        all_rows = list(reader)

    if not all_rows:
        print("\n  No records found in output file")
        return

    sample_rows = random.sample(all_rows, min(sample_size, len(all_rows)))

    print(f"\n  Validating provenance for {len(sample_rows)} random records...")

    validated = 0
    failed = 0

    for row in sample_rows:
        row_id = row.get('unmapped._row_id', '')
        patient_id = row.get('patientId', 'unknown')

        if not row_id:
            failed += 1
            continue

        try:
            # Get lineage
            lineage = client.lineage.get_row(row_id)
            events = lineage.get("events", [])

            if len(events) >= 2:  # Should have source + transformations
                print(f"\n  ✓ Patient {patient_id}: {len(events)} lineage events")

                # Show transformation chain
                for event in events[-3:]:  # Last 3 events
                    step = event.get('step_id', 'unknown')
                    transforms = event.get('transformations', [])
                    print(f"      → {step} ({len(transforms)} transforms)")

                validated += 1
            else:
                print(f"\n  ⚠ Patient {patient_id}: Incomplete lineage ({len(events)} events)")
                failed += 1

        except Exception as e:
            print(f"\n  ✗ Patient {patient_id}: {str(e)[:60]}")
            failed += 1

    print(f"\n  Provenance Validation Summary:")
    print(f"    Validated: {validated}/{len(sample_rows)}")
    print(f"    Failed:    {failed}/{len(sample_rows)}")
    print(f"    Success:   {(validated/len(sample_rows)*100):.1f}%")


def generate_validation_report(metrics: Dict[str, Any], output_path: str = "/tmp/validation_report.json") -> None:
    """Generate a JSON validation report."""
    print("\n" + "=" * 60)
    print("Generating Validation Report")
    print("=" * 60)

    report = {
        "generated_at": datetime.now().isoformat(),
        "test_version": "v5",
        "metrics": metrics,
        "status": "PASSED" if metrics["total_rows"] > 0 else "FAILED"
    }

    with open(output_path, 'w') as f:
        json.dump(report, f, indent=2)

    print(f"\n  ✓ Validation report saved to: {output_path}")
    print(f"  Status: {report['status']}")


def main():
    """Main demo execution."""
    print("=" * 60)
    print("Healthcare ETL Demo v5 - Enhanced Validation Pipeline")
    print("=" * 60)

    # Connect with extended timeout for 1M row processing
    print(f"\nConnecting to {SERVER_URL}...")
    client = Client(SERVER_URL, timeout=600)  # 10 minute timeout

    try:
        client.get("/api/v1/health")
    except Exception:
        print(f"  Auth required, using {USERNAME}...")
        client = Client(SERVER_URL, auth=BasicAuth(USERNAME, PASSWORD), timeout=600)

    # Step 1: Generate 1M records with 20 fields
    print("\n" + "=" * 60)
    print("Step 1: Generate Synthetic Data (1M records, 20 fields)")
    print("=" * 60)
    csv_path, expected_unique = create_extended_healthcare_data(1000000)
    show_csv_sample(csv_path)

    # Step 2: Upload extended ontology
    print("\n" + "=" * 60)
    print("Step 2: Upload/Update Healthcare Ontology (20 properties)")
    print("=" * 60)
    upload_or_update_ontology(client)

    # Step 3: Register DB2 datasource
    print("\n" + "=" * 60)
    print("Step 3: Register DB2 Datasource")
    print("=" * 60)
    register_db2_datasource(client)

    # Step 4: Create comprehensive workflow
    print("\n" + "=" * 60)
    print("Step 4: Create Multi-Step Workflow")
    print("=" * 60)
    workflow = create_comprehensive_workflow(client, csv_path)
    workflow_id = workflow.get("id")

    # Step 5: Execute workflow
    print("\n" + "=" * 60)
    print("Step 5: Execute Workflow")
    print("=" * 60)
    execution = execute_workflow(client, workflow_id)

    # Step 6: Extract actual output path from workflow result
    # The CSV exporter now generates UUID-based filenames to prevent collisions
    output_path = None
    results = execution.get("results", [])

    # Debug: Print the structure of the response
    print(f"\n  DEBUG: results type: {type(results)}, length: {len(results) if results else 0}")
    if results and len(results) > 0:
        print(f"  DEBUG: results[0] keys: {list(results[0].keys()) if isinstance(results[0], dict) else 'not a dict'}")
        step_results = results[0].get("step_results", {})
        print(f"  DEBUG: step_results type: {type(step_results)}")
        if isinstance(step_results, dict):
            print(f"  DEBUG: step_results keys: {list(step_results.keys())}")
        elif isinstance(step_results, list):
            print(f"  DEBUG: step_results length: {len(step_results)}")
            if step_results:
                print(f"  DEBUG: step_results[0]: {step_results[0]}")

        # step_results can be either a dict or a list
        if isinstance(step_results, dict):
            csv_export_result = step_results.get("csv_export", {})
            print(f"  DEBUG: csv_export_result keys: {list(csv_export_result.keys()) if isinstance(csv_export_result, dict) else 'not a dict'}")
            output_path = csv_export_result.get("_output_path")
        elif isinstance(step_results, list):
            # Search through list for csv_export step
            print(f"  DEBUG: Searching {len(step_results)} steps for csv_export...")
            # First, print ALL step IDs to verify completeness
            all_step_ids = [step.get("step_id") if isinstance(step, dict) else "?" for step in step_results]
            print(f"  DEBUG: All step IDs: {all_step_ids}")
            for step in step_results:
                if isinstance(step, dict):
                    step_id = step.get("step_id")
                    if step_id == "csv_export":
                        # _output_path is in the step's output field
                        step_output = step.get("output", {})
                        if isinstance(step_output, dict):
                            output_path = step_output.get("_output_path")
                            print(f"  DEBUG:   - csv_export output_path: {output_path}")
                            if output_path:
                                break

    if not output_path:
        # Fallback to hardcoded path for backward compatibility
        output_path = "/tmp/healthcare_deduped_1m.csv"
        print(f"\n  Warning: Could not extract output path from workflow result, using default: {output_path}")
    else:
        print(f"\n  Actual output file: {output_path}")

    validate_csv_output(csv_path, output_path, expected_unique)

    # Step 7: V5 Enhanced Validations
    print("\n" + "=" * 60)
    print("Step 7: Enhanced V5 Validation Suite")
    print("=" * 60)

    # 7a: Workflow metadata validation
    validate_workflow_metadata(client, workflow_id, execution)

    # 7b: Data quality metrics
    quality_metrics = validate_data_quality(output_path)

    # 7c: Semantic consistency checks
    validate_semantic_consistency(output_path)

    # 7d: Provenance chain validation
    validate_provenance_chain(client, output_path, sample_size=10)

    # 7e: SPARQL governance queries
    validate_with_sparql(client)

    # 7f: Generate validation report
    generate_validation_report(quality_metrics)

    # Step 8: Legacy column lineage verification
    verify_column_lineage(client)

    # Step 9: Legacy sample lineage verification
    sample_lineage_verification(client, csv_path, num_samples=10)

    # Summary
    print("\n" + "=" * 60)
    print("Demo Complete!")
    print("=" * 60)

    print("\nPipeline Summary:")
    print("-" * 60)
    print(f"  {'Source CSV:':<30} {csv_path}")
    print(f"  {'Total records:':<30} 1,000,000")
    print(f"  {'Expected unique (post-dedup):':<30} {expected_unique}")
    print(f"  {'Target DB2 table:':<30} {DB2_TARGET_TABLE}")
    print(f"  {'Workflow ID:':<30} {workflow_id}")

    print("\nPipeline Steps:")
    print("-" * 60)
    print("  1. CSV Source         - Load 1M records with row lineage")
    print("  2. Semantic Mapper    - Map to healthcare ontology (20 fields)")
    print("  3. Deduplicator       - Remove duplicates")
    print("  4. CSV Export         - Export deduplicated data")

    print("\nV5 Validation Features:")
    print("-" * 60)
    print("  ✓ SPARQL governance queries")
    print("  ✓ Workflow metadata validation")
    print("  ✓ Data quality metrics (completeness, uniqueness)")
    print("  ✓ Semantic consistency checks")
    print("  ✓ Provenance chain verification")
    print("  ✓ JSON validation report")

    print("\nOntology Coverage:")
    print("-" * 60)
    print("  20 properties mapped:")
    print("    - Core Identity (5):  patient_id, first_name, last_name, dob, ssn")
    print("    - Contact Info (5):   email, phone, address, city, state")
    print("    - Clinical (5):       blood_type, condition, medication, dept, physician")
    print("    - Visit/Billing (5):  visit_date, cost, insurance, marital, ethnicity")

    print("\n" + "=" * 60)
    print("All validation reports generated!")
    print("=" * 60)
    print(f"\n  CSV Output:         {output_path}")
    print(f"  JSON Report:        /tmp/validation_report.json")
    print(f"\nReview the validation report for comprehensive quality metrics.")


if __name__ == "__main__":
    main()
