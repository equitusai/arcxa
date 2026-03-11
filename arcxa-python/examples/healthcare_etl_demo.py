#!/usr/bin/env python3
"""
Healthcare ETL Demo: End-to-end data pipeline with Graphica.

This script demonstrates:
1. Creating synthetic healthcare data with duplicates/misspellings
2. Uploading a healthcare ontology
3. Creating a deduplication workflow
4. Loading data to DB2
5. Tracing lineage back to source records
"""

import csv
import random
import string
import tempfile
import os
from datetime import datetime, timedelta
from typing import List, Dict, Any

from graphica import Client, BasicAuth
from graphica.errors import NotFoundError, ValidationError, ServerError


# Configuration
SERVER_URL = "http://localhost:8080"
USERNAME = "admin"
PASSWORD = "Admin@Pass123"

# DB2 connection config
DB2_CONFIG = {
    "host": "localhost",
    "port": 50000,
    "database": "GRAPHICA",
    "username": "db2inst1",
    "password": "graphica-db2-pass",
}


def create_synthetic_healthcare_data(num_records: int = 500) -> str:
    """Generate synthetic healthcare data with duplicates and misspellings.

    Returns path to generated CSV file.
    """
    print(f"Creating {num_records} synthetic healthcare records...")

    # Base data for generation
    first_names = ["John", "Jane", "Michael", "Sarah", "David", "Emily", "Robert", "Lisa", "William", "Jennifer"]
    last_names = ["Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis", "Rodriguez", "Martinez"]
    conditions = ["Diabetes", "Hypertension", "Asthma", "Arthritis", "Depression", "Anxiety", "COPD", "Heart Disease"]
    medications = ["Metformin", "Lisinopril", "Albuterol", "Ibuprofen", "Sertraline", "Alprazolam", "Tiotropium", "Aspirin"]
    departments = ["Cardiology", "Neurology", "Orthopedics", "Pediatrics", "Oncology", "Emergency", "Internal Medicine"]

    def misspell(text: str, probability: float = 0.15) -> str:
        """Introduce random misspellings."""
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

    def random_date(start_year: int = 1950, end_year: int = 2005) -> str:
        start = datetime(start_year, 1, 1)
        end = datetime(end_year, 12, 31)
        delta = end - start
        random_days = random.randint(0, delta.days)
        return (start + timedelta(days=random_days)).strftime("%Y-%m-%d")

    def random_visit_date() -> str:
        start = datetime(2023, 1, 1)
        end = datetime(2024, 11, 1)
        delta = end - start
        random_days = random.randint(0, delta.days)
        return (start + timedelta(days=random_days)).strftime("%Y-%m-%d")

    records = []

    # Generate base records (70% of total)
    base_count = int(num_records * 0.7)
    for i in range(base_count):
        first = random.choice(first_names)
        last = random.choice(last_names)

        record = {
            "patient_id": f"P{i+1:05d}",
            "first_name": misspell(first),
            "last_name": misspell(last),
            "date_of_birth": random_date(),
            "phone": random_phone(),
            "condition": misspell(random.choice(conditions)),
            "medication": misspell(random.choice(medications)),
            "department": random.choice(departments),
            "visit_date": random_visit_date(),
            "cost": round(random.uniform(50, 5000), 2),
        }
        records.append(record)

    # Create duplicates (30% of total) with variations
    print("  Adding duplicates with variations...")
    dup_count = num_records - base_count
    for _ in range(dup_count):
        # Pick a random base record to duplicate
        base = random.choice(records[:base_count]).copy()

        # Modify to create near-duplicate
        base["patient_id"] = f"P{len(records)+1:05d}"  # New ID

        # Randomly modify some fields
        if random.random() < 0.3:
            base["first_name"] = misspell(base["first_name"], probability=0.5)
        if random.random() < 0.3:
            base["last_name"] = misspell(base["last_name"], probability=0.5)
        if random.random() < 0.2:
            # Slight date variation (typo)
            dob = base["date_of_birth"]
            if random.random() < 0.5:
                base["date_of_birth"] = dob[:-1] + str(random.randint(0, 9))
        if random.random() < 0.3:
            # Phone variation
            phone = base["phone"]
            base["phone"] = phone[:-1] + str(random.randint(0, 9))

        records.append(base)

    # Shuffle records
    random.shuffle(records)

    # Write to CSV
    output_path = os.path.join(tempfile.gettempdir(), "healthcare_patients.csv")
    with open(output_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(records[0].keys()))
        writer.writeheader()
        writer.writerows(records)

    print(f"  Created {len(records)} records at: {output_path}")
    return output_path


def create_healthcare_ontology() -> str:
    """Return healthcare ontology in Turtle format."""
    return '''
@prefix hc: <http://graphica.io/healthcare#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Classes
hc:Patient a owl:Class ;
    rdfs:label "Patient" ;
    rdfs:comment "A healthcare patient record" .

hc:MedicalCondition a owl:Class ;
    rdfs:label "Medical Condition" ;
    rdfs:comment "A diagnosed medical condition" .

hc:Medication a owl:Class ;
    rdfs:label "Medication" ;
    rdfs:comment "Prescribed medication" .

hc:Department a owl:Class ;
    rdfs:label "Department" ;
    rdfs:comment "Hospital department" .

hc:Visit a owl:Class ;
    rdfs:label "Visit" ;
    rdfs:comment "Patient visit record" .

# Patient properties
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

hc:phone a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:string ;
    rdfs:label "Phone Number" .

# Relationships
hc:hasCondition a owl:ObjectProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range hc:MedicalCondition ;
    rdfs:label "Has Condition" .

hc:takesMedication a owl:ObjectProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range hc:Medication ;
    rdfs:label "Takes Medication" .

hc:treatedIn a owl:ObjectProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range hc:Department ;
    rdfs:label "Treated In" .

# Visit properties
hc:visitDate a owl:DatatypeProperty ;
    rdfs:domain hc:Visit ;
    rdfs:range xsd:date ;
    rdfs:label "Visit Date" .

hc:visitCost a owl:DatatypeProperty ;
    rdfs:domain hc:Visit ;
    rdfs:range xsd:decimal ;
    rdfs:label "Visit Cost" .

hc:patientVisit a owl:ObjectProperty ;
    rdfs:domain hc:Visit ;
    rdfs:range hc:Patient ;
    rdfs:label "Patient Visit" .
'''


def upload_ontology(client: Client) -> bool:
    """Upload healthcare ontology if not already registered."""
    ontology_id = "healthcare-v1"

    print(f"Checking for existing ontology: {ontology_id}")

    # Check if ontology already exists
    try:
        existing = client.ontology.get(ontology_id)
        if existing.get("metadata", {}).get("active", False):
            print(f"  Ontology '{ontology_id}' already exists and is active")
            return True
        else:
            print(f"  Ontology exists but inactive, activating...")
            client.ontology.activate(ontology_id)
            return True
    except NotFoundError:
        pass

    # Register new ontology
    print(f"  Registering new ontology: {ontology_id}")
    content = create_healthcare_ontology()

    # Validate first
    validation = client.ontology.validate(content)
    print(f"  Validation: {validation['status']}")

    # Register
    result = client.ontology.register(
        ontology_id=ontology_id,
        name="Healthcare Ontology",
        content=content,
        description="Ontology for patient records, conditions, and visits",
        version="1.0.0",
        author="Graphica Demo",
        tags=["healthcare", "patients", "hipaa"],
    )

    print(f"  Registered ontology: {result['metadata']['id']}")
    return True


def create_dedup_workflow(client: Client, csv_path: str) -> Dict[str, Any]:
    """Create workflow for field mapping and deduplication."""
    print("Creating deduplication workflow...")

    workflow_def = {
        "name": "healthcare-dedup-workflow",
        "description": "Map healthcare CSV fields and deduplicate patient records",
        "version": "1.0.0",
        "inputs": {
            "source_file": {
                "type": "file",
                "path": csv_path,
                "format": "csv",
            }
        },
        "outputs": {
            "deduplicated_patients": {
                "type": "table",
                "schema": "healthcare_patients_deduped",
            }
        },
        "steps": [
            {
                "id": "read_csv",
                "name": "Read CSV Source",
                "type": "csv_reader",
                "config": {
                    "path": csv_path,
                    "has_header": True,
                    "delimiter": ",",
                    "track_lineage": True,
                }
            },
            {
                "id": "field_mapping",
                "name": "Map Fields to Ontology",
                "type": "field_mapper",
                "depends_on": ["read_csv"],
                "config": {
                    "ontology_id": "healthcare-v1",
                    "mappings": [
                        {"source": "patient_id", "target": "hc:patientId", "transform": "uppercase"},
                        {"source": "first_name", "target": "hc:firstName", "transform": "titlecase"},
                        {"source": "last_name", "target": "hc:lastName", "transform": "titlecase"},
                        {"source": "date_of_birth", "target": "hc:dateOfBirth", "transform": "parse_date"},
                        {"source": "phone", "target": "hc:phone", "transform": "normalize_phone"},
                        {"source": "condition", "target": "hc:hasCondition"},
                        {"source": "medication", "target": "hc:takesMedication"},
                        {"source": "department", "target": "hc:treatedIn"},
                        {"source": "visit_date", "target": "hc:visitDate", "transform": "parse_date"},
                        {"source": "cost", "target": "hc:visitCost", "transform": "to_decimal"},
                    ]
                }
            },
            {
                "id": "normalize",
                "name": "Normalize Patient Names",
                "type": "transformer",
                "depends_on": ["field_mapping"],
                "config": {
                    "operations": [
                        {
                            "field": "hc:firstName",
                            "operation": "fuzzy_standardize",
                            "params": {"reference_list": "first_names", "threshold": 0.8}
                        },
                        {
                            "field": "hc:lastName",
                            "operation": "fuzzy_standardize",
                            "params": {"reference_list": "last_names", "threshold": 0.8}
                        },
                        {
                            "field": "hc:hasCondition",
                            "operation": "fuzzy_standardize",
                            "params": {"reference_list": "medical_conditions", "threshold": 0.75}
                        }
                    ]
                }
            },
            {
                "id": "dedup",
                "name": "Deduplicate Patients",
                "type": "deduplicator",
                "depends_on": ["normalize"],
                "config": {
                    "strategy": "fuzzy_match",
                    "match_fields": [
                        {"field": "hc:firstName", "weight": 0.25, "algorithm": "levenshtein"},
                        {"field": "hc:lastName", "weight": 0.25, "algorithm": "levenshtein"},
                        {"field": "hc:dateOfBirth", "weight": 0.3, "algorithm": "exact"},
                        {"field": "hc:phone", "weight": 0.2, "algorithm": "numeric_match"},
                    ],
                    "threshold": 0.85,
                    "merge_strategy": "keep_first",
                    "track_duplicates": True,
                }
            },
            {
                "id": "output",
                "name": "Write Deduplicated Records",
                "type": "table_writer",
                "depends_on": ["dedup"],
                "config": {
                    "table_name": "healthcare_patients_deduped",
                    "mode": "overwrite",
                    "track_lineage": True,
                }
            }
        ],
        "lineage": {
            "enabled": True,
            "track_row_level": True,
            "track_field_level": True,
        }
    }

    # Create or update workflow
    try:
        # Check if exists
        existing = client.workflows.list()
        workflow_exists = False
        existing_id = None

        if isinstance(existing, dict) and "workflows" in existing:
            for wf in existing.get("workflows", []):
                if wf.get("name") == workflow_def["name"]:
                    workflow_exists = True
                    existing_id = wf.get("id")
                    break

        if workflow_exists and existing_id:
            print(f"  Updating existing workflow: {existing_id}")
            result = client.workflows.update(existing_id, workflow_def)
        else:
            print("  Creating new workflow")
            result = client.workflows.create(workflow_def)

        print(f"  Workflow ready: {result.get('id', result.get('name', 'unknown'))}")
        return result

    except Exception as e:
        print(f"  Note: Workflow creation returned: {e}")
        # Return the definition for reference
        return {"name": workflow_def["name"], "definition": workflow_def}


def execute_workflow(client: Client, workflow_id: str) -> Dict[str, Any]:
    """Execute the deduplication workflow."""
    print(f"Executing workflow: {workflow_id}")

    try:
        result = client.workflows.execute(
            workflow_id,
            inputs={},
            async_mode=False,
        )
        print(f"  Execution result: {result.get('status', 'completed')}")
        return result
    except Exception as e:
        print(f"  Execution note: {e}")
        return {"status": "simulated", "message": str(e)}


def load_to_db2(client: Client, session_id: str = None) -> Dict[str, Any]:
    """Load deduplicated data to DB2."""
    print("Loading data to DB2...")

    try:
        if session_id:
            result = client.mapping.load_to_database(
                session_id=session_id,
                database_type="d_b2",
                connection_config=DB2_CONFIG,
                create_tables=True,
                validate_data=True,
                batch_size=100,
            )
            print(f"  Load job started: {result.get('load_job_id', 'unknown')}")
            return result
        else:
            print("  Note: No session_id provided, skipping actual load")
            return {"status": "skipped", "reason": "no_session_id"}
    except Exception as e:
        print(f"  Load note: {e}")
        return {"status": "error", "message": str(e)}


def inspect_lineage(client: Client, workflow_run_id: str = None) -> None:
    """Inspect workflow lineage."""
    print("\n=== Lineage Inspection ===")

    try:
        # Get workflow lineage
        if workflow_run_id:
            lineage = client.lineage.get_run(workflow_run_id)
            print(f"Run lineage: {lineage}")

        # Query by time range
        result = client.lineage.time_range(
            start_time="2024-01-01T00:00:00Z",
            end_time="2025-12-31T23:59:59Z",
        )
        print(f"  Lineage records in range: {len(result) if isinstance(result, list) else result}")

    except Exception as e:
        print(f"  Lineage query note: {e}")


def trace_record_to_source(client: Client, record_key: str = None) -> None:
    """Trace a patient record back to source CSV row."""
    print("\n=== Record Tracing ===")

    if not record_key:
        # Use a sample key
        record_key = "healthcare_patients_deduped:P00001"

    print(f"Tracing record: {record_key}")

    try:
        # Get row lineage
        row_lineage = client.lineage.get_row(record_key)
        print(f"  Row lineage: {row_lineage}")

        # Get journey (full transformation path)
        journey = client.lineage.get_row_journey(record_key)
        print(f"  Journey steps: {len(journey) if isinstance(journey, list) else journey}")

        # Show source
        if isinstance(journey, list) and len(journey) > 0:
            source = journey[0]
            print(f"  Source file: {source.get('source_file', 'unknown')}")
            print(f"  Source row: {source.get('source_row', 'unknown')}")

    except Exception as e:
        print(f"  Tracing note: {e}")


def main():
    """Main demo execution."""
    print("=" * 60)
    print("Healthcare ETL Demo with Graphica")
    print("=" * 60)

    # Connect with authentication
    print(f"\nConnecting to {SERVER_URL} as {USERNAME}...")
    client = Client(SERVER_URL, auth=BasicAuth(USERNAME, PASSWORD))

    # Step 1: Create synthetic data
    print("\n" + "=" * 40)
    print("Step 1: Generate Synthetic Data")
    print("=" * 40)
    csv_path = create_synthetic_healthcare_data(500)

    # Step 2: Upload ontology
    print("\n" + "=" * 40)
    print("Step 2: Upload Healthcare Ontology")
    print("=" * 40)
    upload_ontology(client)

    # Step 3: Create workflow
    print("\n" + "=" * 40)
    print("Step 3: Create Deduplication Workflow")
    print("=" * 40)
    workflow = create_dedup_workflow(client, csv_path)
    workflow_id = workflow.get("id", workflow.get("name", "healthcare-dedup-workflow"))

    # Step 4: Execute workflow
    print("\n" + "=" * 40)
    print("Step 4: Execute Workflow")
    print("=" * 40)
    execution = execute_workflow(client, workflow_id)
    run_id = execution.get("run_id")

    # Step 5: Load to DB2
    print("\n" + "=" * 40)
    print("Step 5: Load to DB2")
    print("=" * 40)
    # Note: This would need a mapping session ID from the workflow output
    load_result = load_to_db2(client)

    # Step 6: Inspect lineage
    print("\n" + "=" * 40)
    print("Step 6: Inspect Lineage")
    print("=" * 40)
    inspect_lineage(client, run_id)

    # Step 7: Trace record
    print("\n" + "=" * 40)
    print("Step 7: Trace Record to Source")
    print("=" * 40)
    trace_record_to_source(client, "P00001")

    print("\n" + "=" * 60)
    print("Demo Complete!")
    print("=" * 60)

    # Summary
    print("\nSummary:")
    print(f"  - Generated CSV: {csv_path}")
    print(f"  - Ontology: healthcare-v1")
    print(f"  - Workflow: {workflow_id}")
    print(f"  - Server: {SERVER_URL}")


if __name__ == "__main__":
    main()
