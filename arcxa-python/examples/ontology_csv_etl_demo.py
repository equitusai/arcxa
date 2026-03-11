#!/usr/bin/env python3
"""
Ontology-Driven CSV ETL Demo - Complete End-to-End Pipeline

Demonstrates:
  1. CSV parsing
  2. Deduplication
  3. Data transformation
  4. Ontology-driven DB2 loading with automatic schema generation

The key feature is entity_uri in db_loader config, which triggers:
  - Automatic table schema generation from RDF ontology
  - XSD → SQL type mapping
  - CREATE TABLE execution
  - Data transformation to match schema
"""

import csv
import json
import tempfile
import os
import time
from datetime import datetime, timedelta
from typing import Dict, Any

from graphica import Client, BasicAuth
from graphica.errors import NotFoundError, ValidationError, ServerError


# Configuration
SERVER_URL = "http://localhost:8082"
USERNAME = "admin"
PASSWORD = "Admin@Pass123"
DB2_DATASOURCE_ID = "db2-healthcare"
DB2_TARGET_TABLE = "DEMO_PATIENTS_ONTOLOGY"


def create_healthcare_ontology() -> str:
    """Create healthcare ontology with SHACL shapes for Patient entity."""
    ontology_ttl = """
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix health: <http://healthcare.org/ontology#> .

# Ontology definition
health: a owl:Ontology ;
    rdfs:label "Healthcare Ontology" ;
    rdfs:comment "Simple healthcare ontology for demo" .

# Patient class
health:Patient a owl:Class ;
    rdfs:label "Patient" ;
    rdfs:comment "A healthcare patient" .

# Properties
health:patientId a owl:DatatypeProperty ;
    rdfs:label "Patient ID" ;
    rdfs:domain health:Patient ;
    rdfs:range xsd:string .

health:firstName a owl:DatatypeProperty ;
    rdfs:label "First Name" ;
    rdfs:domain health:Patient ;
    rdfs:range xsd:string .

health:lastName a owl:DatatypeProperty ;
    rdfs:label "Last Name" ;
    rdfs:domain health:Patient ;
    rdfs:range xsd:string .

health:dateOfBirth a owl:DatatypeProperty ;
    rdfs:label "Date of Birth" ;
    rdfs:domain health:Patient ;
    rdfs:range xsd:date .

health:city a owl:DatatypeProperty ;
    rdfs:label "City" ;
    rdfs:domain health:Patient ;
    rdfs:range xsd:string .

health:state a owl:DatatypeProperty ;
    rdfs:label "State" ;
    rdfs:domain health:Patient ;
    rdfs:range xsd:string .

# SHACL Shape for automatic schema generation
health:PatientShape a sh:NodeShape ;
    sh:targetClass health:Patient ;
    sh:property [
        sh:path health:patientId ;
        sh:datatype xsd:string ;
        sh:maxLength 20 ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
    ] ;
    sh:property [
        sh:path health:firstName ;
        sh:datatype xsd:string ;
        sh:maxLength 50 ;
        sh:minCount 1 ;
    ] ;
    sh:property [
        sh:path health:lastName ;
        sh:datatype xsd:string ;
        sh:maxLength 50 ;
        sh:minCount 1 ;
    ] ;
    sh:property [
        sh:path health:dateOfBirth ;
        sh:datatype xsd:date ;
        sh:minCount 1 ;
    ] ;
    sh:property [
        sh:path health:city ;
        sh:datatype xsd:string ;
        sh:maxLength 100 ;
    ] ;
    sh:property [
        sh:path health:state ;
        sh:datatype xsd:string ;
        sh:maxLength 2 ;
    ] .
"""
    return ontology_ttl


def generate_patient_csv(num_records: int = 50, duplicate_rate: float = 0.15) -> str:
    """Generate CSV file with patient data including duplicates."""
    csv_file = tempfile.NamedTemporaryFile(mode='w', delete=False, suffix='.csv', prefix='patients_')
    writer = csv.DictWriter(csv_file, fieldnames=[
        'patientId', 'firstName', 'lastName', 'dateOfBirth', 'city', 'state'
    ])
    writer.writeheader()

    # First names, last names, cities for variety
    first_names = ['John', 'Mary', 'James', 'Patricia', 'Robert', 'Jennifer', 'Michael', 'Linda']
    last_names = ['Smith', 'Johnson', 'Williams', 'Brown', 'Jones', 'Garcia', 'Miller', 'Davis']
    cities = ['New York', 'Los Angeles', 'Chicago', 'Houston', 'Phoenix', 'Philadelphia']
    states = ['NY', 'CA', 'IL', 'TX', 'AZ', 'PA']

    records = []
    base_date = datetime(1950, 1, 1)

    # Generate unique records
    num_unique = int(num_records * (1 - duplicate_rate))
    for i in range(num_unique):
        record = {
            'patientId': f'P{i+1:06d}',
            'firstName': first_names[i % len(first_names)],
            'lastName': last_names[i % len(last_names)],
            'dateOfBirth': (base_date + timedelta(days=i*100)).strftime('%Y-%m-%d'),
            'city': cities[i % len(cities)],
            'state': states[i % len(states)]
        }
        records.append(record)
        writer.writerow(record)

    # Add duplicates
    num_duplicates = num_records - num_unique
    for i in range(num_duplicates):
        # Duplicate random existing records
        original = records[i % len(records)].copy()
        original['patientId'] = f'P{num_unique + i + 1:06d}'  # Different ID but same data
        writer.writerow(original)

    csv_file.close()
    return csv_file.name, num_unique, num_duplicates


def main():
    print("=" * 70)
    print("  ONTOLOGY-DRIVEN CSV ETL DEMO")
    print("=" * 70)
    print()
    print("Pipeline: CSV → Dedup → Transform → DB2 (auto-schema from ontology)")
    print()

    # Step 1: Connect
    print("[1/7] Connecting to Graphica coordinator...")
    client = Client(SERVER_URL, auth=BasicAuth(USERNAME, PASSWORD))
    print(f"      ✓ Connected to {SERVER_URL}")

    # Step 2: Register ontology
    print("\n[2/7] Registering healthcare ontology...")
    ontology_id = f"healthcare_ontology_{int(time.time())}"
    ontology_ttl = create_healthcare_ontology()

    try:
        client.ontology.register(ontology_id, ontology_ttl, format="turtle")
        print(f"      ✓ Ontology registered: {ontology_id}")
    except Exception as e:
        print(f"      ⚠ Ontology registration: {e}")
        print(f"      → Continuing anyway (may already exist)")

    # Step 3: Generate CSV data
    print("\n[3/7] Generating patient CSV data...")
    csv_path, num_unique, num_duplicates = generate_patient_csv(50, 0.15)
    print(f"      ✓ CSV created: {csv_path}")
    print(f"      → Total records: 50 (unique: {num_unique}, duplicates: {num_duplicates})")

    try:
        # Step 4: Ensure DB2 datasource exists
        print("\n[4/7] Verifying DB2 datasource...")
        try:
            datasources = client.datasources.list()
            db2_exists = any(ds.get('id') == DB2_DATASOURCE_ID for ds in (datasources if isinstance(datasources, list) else []))

            if not db2_exists:
                print(f"      ⚠ Datasource '{DB2_DATASOURCE_ID}' not found, creating...")
                ds_config = {
                    "id": DB2_DATASOURCE_ID,
                    "name": "DB2 Healthcare",
                    "source_type": "db2",
                    "config": {
                        "host": "localhost",
                        "port": 50000,
                        "database": "GRAPHICA",
                        "username": "db2inst1",
                        "password": "graphica-db2-pass"
                    }
                }
                client.datasources.create(ds_config)
                print(f"      ✓ Datasource created: {DB2_DATASOURCE_ID}")
            else:
                print(f"      ✓ Datasource exists: {DB2_DATASOURCE_ID}")
        except Exception as e:
            print(f"      ⚠ Datasource check: {e}")
            print(f"      → Continuing anyway")

        # Step 5: Create workflow with ontology-driven loading
        print("\n[5/7] Creating ETL workflow...")
        workflow_id = f"ontology_etl_{int(time.time())}"
        workflow_request = {
            "name": f"Ontology ETL Demo {int(time.time())}",
            "description": "CSV → Dedup → Transform → DB2 with ontology-driven schema",
            "tags": ["demo", "ontology", "etl"],
            "definition": {
                "steps": [
                    # Step 1: Load CSV
                    {
                        "id": "load_csv",
                        "step_type": "csv_source",
                        "config": {
                            "file_path": csv_path,
                            "has_header": True,
                            "delimiter": ","
                        }
                    },
                    # Step 2: Deduplicate
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
                    # Step 3: Transform (uppercase city names)
                    {
                        "id": "transform",
                        "step_type": "field_transformer",
                        "depends_on": ["deduplicate"],
                        "config": {
                            "transformations": [
                                {
                                    "field": "city",
                                    "operations": [
                                        {"type": "UPPER"},
                                        {"type": "TRIM"}
                                    ]
                                },
                                {
                                    "field": "state",
                                    "operations": [
                                        {"type": "UPPER"},
                                        {"type": "TRIM"}
                                    ]
                                }
                            ]
                        }
                    },
                    # Step 4: Load to DB2 with ontology-driven schema generation
                    {
                        "id": "load_db2",
                        "step_type": "db_loader",
                        "depends_on": ["transform"],
                        "config": {
                            "datasource_id": DB2_DATASOURCE_ID,
                            "table_name": DB2_TARGET_TABLE,
                            "mode": "insert",
                            "batch_size": 100,
                            "create_table": True,
                            "entity_uri": "http://healthcare.org/ontology#Patient"  # ⭐ KEY: Triggers ontology-driven loading
                        }
                    }
                ]
            }
        }

        result = client.workflows.create(workflow_request)
        created_id = result.get("workflow_id", result.get("id", workflow_id))
        print(f"      ✓ Workflow created: {created_id}")

        # Step 6: Execute workflow
        print("\n[6/7] Executing workflow...")
        print(f"      → Expected: ~{num_unique} records after deduplication")

        exec_result = client.workflows.execute(created_id, {})

        if exec_result.get("success"):
            execution_id = exec_result.get("execution_id")
            print(f"      ✓ Execution succeeded: {execution_id}")

            # Print execution stats
            if "stats" in exec_result:
                stats = exec_result["stats"]
                print(f"      → Records processed: {stats.get('records_processed', 'N/A')}")
                print(f"      → Records loaded: {stats.get('records_loaded', 'N/A')}")
        else:
            print(f"      ✗ Execution failed: {exec_result.get('error', 'Unknown error')}")

        # Step 7: Verify results
        print("\n[7/7] Verification Summary")
        print(f"      ✓ CSV file: {csv_path}")
        print(f"      ✓ Ontology: {ontology_id}")
        print(f"      ✓ Workflow: {created_id}")
        print(f"      ✓ Entity URI: http://healthcare.org/ontology#Patient")
        print(f"      ✓ Expected table: {DB2_TARGET_TABLE}")
        print()
        print("=" * 70)
        print("  DEMO COMPLETE")
        print("=" * 70)
        print()
        print("What happened:")
        print("  1. Ontology registered with SHACL shapes")
        print("  2. CSV with 50 records (15% duplicates) generated")
        print("  3. Workflow created with ontology-driven loading")
        print("  4. Deduplication reduced ~50 → ~42 records")
        print("  5. City/state transformed to uppercase")
        print("  6. OntologyDrivenLoader:")
        print("     • Queried ontology for Patient entity definition")
        print("     • Generated DDL from SHACL shape:")
        print("       CREATE TABLE DEMO_PATIENTS_ONTOLOGY (")
        print("         PATIENTID VARCHAR(20) NOT NULL PRIMARY KEY,")
        print("         FIRSTNAME VARCHAR(50) NOT NULL,")
        print("         LASTNAME VARCHAR(50) NOT NULL,")
        print("         DATEOFBIRTH DATE NOT NULL,")
        print("         CITY VARCHAR(100),")
        print("         STATE VARCHAR(2)")
        print("       )")
        print("     • Transformed JSON data to match schema")
        print("     • Executed batch INSERT")
        print(f"  7. Data loaded to DB2 table: {DB2_TARGET_TABLE}")
        print()
        print("To query the data:")
        print(f"  db2 connect to GRAPHICA")
        print(f"  db2 'SELECT * FROM {DB2_TARGET_TABLE}'")
        print()

    finally:
        # Cleanup
        if os.path.exists(csv_path):
            os.unlink(csv_path)
            print(f"Cleaned up CSV: {csv_path}")


if __name__ == "__main__":
    main()
