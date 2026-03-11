#!/usr/bin/env python3
"""
Healthcare ETL Demo v2: End-to-end data pipeline with Graphica.

Uses correct workflow API schema based on OpenAPI spec.
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

# DB2 datasource ID (should be registered in Graphica)
DB2_DATASOURCE_ID = "db2-healthcare"


def create_synthetic_healthcare_data(num_records: int = 500) -> str:
    """Generate synthetic healthcare data with duplicates and misspellings."""
    print(f"Creating {num_records} synthetic healthcare records...")

    first_names = ["John", "Jane", "Michael", "Sarah", "David", "Emily", "Robert", "Lisa", "William", "Jennifer"]
    last_names = ["Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis", "Rodriguez", "Martinez"]
    conditions = ["Diabetes", "Hypertension", "Asthma", "Arthritis", "Depression", "Anxiety", "COPD", "Heart Disease"]
    medications = ["Metformin", "Lisinopril", "Albuterol", "Ibuprofen", "Sertraline", "Alprazolam", "Tiotropium", "Aspirin"]
    departments = ["Cardiology", "Neurology", "Orthopedics", "Pediatrics", "Oncology", "Emergency", "Internal Medicine"]

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

    records = []

    # Generate base records (70%)
    base_count = int(num_records * 0.7)
    for i in range(base_count):
        record = {
            "patient_id": f"P{i+1:05d}",
            "first_name": misspell(random.choice(first_names)),
            "last_name": misspell(random.choice(last_names)),
            "date_of_birth": random_date(),
            "phone": random_phone(),
            "condition": misspell(random.choice(conditions)),
            "medication": misspell(random.choice(medications)),
            "department": random.choice(departments),
            "visit_date": random_visit_date(),
            "cost": round(random.uniform(50, 5000), 2),
        }
        records.append(record)

    # Create duplicates (30%) with variations
    print("  Adding duplicates with variations...")
    for _ in range(num_records - base_count):
        base = random.choice(records[:base_count]).copy()
        base["patient_id"] = f"P{len(records)+1:05d}"
        if random.random() < 0.3:
            base["first_name"] = misspell(base["first_name"], probability=0.5)
        if random.random() < 0.3:
            base["last_name"] = misspell(base["last_name"], probability=0.5)
        if random.random() < 0.2:
            dob = base["date_of_birth"]
            base["date_of_birth"] = dob[:-1] + str(random.randint(0, 9))
        if random.random() < 0.3:
            phone = base["phone"]
            base["phone"] = phone[:-1] + str(random.randint(0, 9))
        records.append(base)

    random.shuffle(records)

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

hc:Patient a owl:Class ;
    rdfs:label "Patient" ;
    rdfs:comment "A healthcare patient record" .

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

hc:visitDate a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:date ;
    rdfs:label "Visit Date" .

hc:visitCost a owl:DatatypeProperty ;
    rdfs:domain hc:Patient ;
    rdfs:range xsd:decimal ;
    rdfs:label "Visit Cost" .
'''


def upload_ontology(client: Client) -> bool:
    """Upload healthcare ontology if not already registered."""
    ontology_id = "healthcare-v1"
    print(f"Checking for existing ontology: {ontology_id}")

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

    print(f"  Registering new ontology: {ontology_id}")
    content = create_healthcare_ontology()
    validation = client.ontology.validate(content)
    print(f"  Validation: {validation['status']}")

    result = client.ontology.register(
        ontology_id=ontology_id,
        name="Healthcare Ontology",
        content=content,
        description="Ontology for patient records",
        version="1.0.0",
        author="Graphica Demo",
        tags=["healthcare", "patients"],
    )
    print(f"  Registered ontology: {result['metadata']['id']}")
    return True


def create_dedup_workflow(client: Client, csv_path: str) -> Dict[str, Any]:
    """Create workflow with correct API schema."""
    print("Creating deduplication workflow...")

    # Workflow definition matching the API schema (untagged union for config)
    workflow_request = {
        "name": "healthcare-dedup-workflow",
        "description": "Deduplicate healthcare patient records from CSV",
        "tags": ["healthcare", "dedup", "etl"],
        "definition": {
            "steps": [
                {
                    "id": "csv_source",
                    "step_type": "csv_source",
                    "config": {
                        "file_path": csv_path,
                        "has_header": True,
                        "delimiter": ",",
                    }
                },
                {
                    "id": "dedup",
                    "step_type": "deduplicator",
                    "depends_on": ["csv_source"],
                    "config": {
                        "method": {
                            "fuzzy": {
                                "algorithm": "levenshtein"
                            }
                        },
                        "key_fields": ["first_name", "last_name", "date_of_birth"],
                        "threshold": 0.85,
                        "keep": "first"
                    }
                },
                {
                    "id": "db_load",
                    "step_type": "db_loader",
                    "depends_on": ["dedup"],
                    "config": {
                        "datasource_id": DB2_DATASOURCE_ID,
                        "table_name": "healthcare_patients_deduped",
                        "create_table": True,
                        "batch_size": 100,
                        "mode": "upsert",
                        "key_fields": ["patient_id"]
                    }
                }
            ]
        }
    }

    try:
        result = client.workflows.create(workflow_request)
        workflow_id = result.get("id", result.get("name", "unknown"))
        print(f"  Created workflow: {workflow_id}")
        return result
    except ValidationError as e:
        print(f"  Validation error: {e}")
        return {"name": workflow_request["name"], "error": str(e)}
    except Exception as e:
        print(f"  Note: {e}")
        return {"name": workflow_request["name"], "error": str(e)}


def execute_workflow(client: Client, workflow_id: str) -> Dict[str, Any]:
    """Execute the workflow."""
    print(f"Executing workflow: {workflow_id}")

    try:
        result = client.workflows.execute(workflow_id, inputs={})
        status = result.get("status", "unknown")
        run_id = result.get("execution_id", result.get("run_id", "unknown"))
        print(f"  Status: {status}")
        print(f"  Execution ID: {run_id}")
        return result
    except Exception as e:
        print(f"  Note: {e}")
        return {"status": "error", "message": str(e)}


def inspect_lineage(client: Client, execution_id: str = None) -> None:
    """Inspect workflow lineage."""
    print("\n=== Lineage Inspection ===")

    # List recent lineage records
    try:
        if execution_id:
            print(f"Getting lineage for execution: {execution_id}")
            run_lineage = client.lineage.get_run(execution_id)
            print(f"  Run records: {len(run_lineage) if isinstance(run_lineage, list) else run_lineage}")
    except Exception as e:
        print(f"  Note: {e}")

    # Check column lineage for a specific field
    try:
        print("\nColumn lineage for 'first_name':")
        col_lineage = client.lineage.get_column("healthcare_patients_deduped", "first_name")
        print(f"  Sources: {col_lineage}")
    except Exception as e:
        print(f"  Note: {e}")


def trace_record(client: Client, patient_id: str) -> None:
    """Trace a patient record back to source CSV row."""
    print(f"\n=== Tracing Record: {patient_id} ===")

    # Format: table:key
    row_key = f"healthcare_patients_deduped:{patient_id}"

    try:
        # Get row lineage
        row_lineage = client.lineage.get_row(row_key)
        print(f"Row lineage: {row_lineage}")

        # Get full journey
        journey = client.lineage.get_row_journey(row_key)
        if isinstance(journey, list) and len(journey) > 0:
            print("\nTransformation journey:")
            for i, step in enumerate(journey):
                print(f"  {i+1}. {step.get('step', step.get('type', 'unknown'))}")
                if step.get("source_file"):
                    print(f"     Source: {step['source_file']}")
                if step.get("source_row"):
                    print(f"     Row: {step['source_row']}")
        else:
            print(f"Journey: {journey}")

    except Exception as e:
        print(f"  Note: {e}")


def show_csv_sample(csv_path: str, num_rows: int = 5) -> None:
    """Display sample rows from CSV."""
    print(f"\nSample data from {csv_path}:")
    with open(csv_path, "r") as f:
        reader = csv.DictReader(f)
        for i, row in enumerate(reader):
            if i >= num_rows:
                break
            print(f"  Row {i+1}: {row['patient_id']} - {row['first_name']} {row['last_name']} ({row['date_of_birth']})")


def main():
    """Main demo execution."""
    print("=" * 60)
    print("Healthcare ETL Demo v2")
    print("=" * 60)

    # Connect
    print(f"\nConnecting to {SERVER_URL} as {USERNAME}...")
    client = Client(SERVER_URL, auth=BasicAuth(USERNAME, PASSWORD))

    # Step 1: Create data
    print("\n" + "=" * 40)
    print("Step 1: Generate Synthetic Data")
    print("=" * 40)
    csv_path = create_synthetic_healthcare_data(500)
    show_csv_sample(csv_path)

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
    workflow_id = workflow.get("id", "healthcare-dedup-workflow")

    # Step 4: Execute workflow
    print("\n" + "=" * 40)
    print("Step 4: Execute Workflow")
    print("=" * 40)
    execution = execute_workflow(client, workflow_id)
    execution_id = execution.get("execution_id")

    # Step 5: Inspect lineage
    print("\n" + "=" * 40)
    print("Step 5: Inspect Lineage")
    print("=" * 40)
    inspect_lineage(client, execution_id)

    # Step 6: Trace record
    print("\n" + "=" * 40)
    print("Step 6: Trace Record to Source")
    print("=" * 40)
    trace_record(client, "P00001")

    print("\n" + "=" * 60)
    print("Demo Complete!")
    print("=" * 60)
    print(f"\nGenerated CSV: {csv_path}")


if __name__ == "__main__":
    main()
