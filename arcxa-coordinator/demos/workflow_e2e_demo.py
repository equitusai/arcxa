#!/usr/bin/env python3
"""
End-to-End Workflow API Demo

This script demonstrates Graphica's complete workflow capabilities:
1. Data Creation - Upload CSV file with customer data
2. Pipeline Processing - Create workflow with multiple actions
3. Ontology Alignment - Map CSV fields to ontological classes
4. Transformation - Parse CSV and transform data
5. Deduplication - Remove duplicate records
6. Lineage Tracing - Track data provenance
7. DB2 Loading - Migrate to DB2 instance

Prerequisites:
- Graphica coordinator running on localhost:8080
- DB2 instance running (for final migration step)
- Python 3.8+
"""

import json
import requests
import sys
import time
from pathlib import Path
from typing import Dict, Any, Optional

# Configuration
COORDINATOR_BASE_URL = "http://localhost:8080"
API_BASE = f"{COORDINATOR_BASE_URL}/api/v1"
WORKFLOW_API_BASE = f"{COORDINATOR_BASE_URL}/api/v2/workflows"

# Color output helpers
class Colors:
    HEADER = '\033[95m'
    OKBLUE = '\033[94m'
    OKCYAN = '\033[96m'
    OKGREEN = '\033[92m'
    WARNING = '\033[93m'
    FAIL = '\033[91m'
    ENDC = '\033[0m'
    BOLD = '\033[1m'

def print_header(msg: str):
    print(f"\n{Colors.HEADER}{Colors.BOLD}{'='*80}{Colors.ENDC}")
    print(f"{Colors.HEADER}{Colors.BOLD}{msg}{Colors.ENDC}")
    print(f"{Colors.HEADER}{Colors.BOLD}{'='*80}{Colors.ENDC}\n")

def print_success(msg: str):
    print(f"{Colors.OKGREEN}✓ {msg}{Colors.ENDC}")

def print_info(msg: str):
    print(f"{Colors.OKCYAN}ℹ {msg}{Colors.ENDC}")

def print_warning(msg: str):
    print(f"{Colors.WARNING}⚠ {msg}{Colors.ENDC}")

def print_error(msg: str):
    print(f"{Colors.FAIL}✗ {msg}{Colors.ENDC}")

def print_json(data: Any, label: str = "Response"):
    print(f"{Colors.OKBLUE}{label}:{Colors.ENDC}")
    print(json.dumps(data, indent=2))

# ============================================================================
# Step 1: Upload CSV Data via File Library
# ============================================================================

def step1_upload_csv_data() -> str:
    """Upload customer data CSV file and return file_id"""
    print_header("STEP 1: Upload CSV Data via File Library")

    # Create sample customer data
    csv_content = """customer_id,first_name,last_name,email,phone,country,registration_date
1001,John,Doe,john.doe@example.com,+1-555-0101,USA,2024-01-15
1002,Jane,Smith,jane.smith@example.com,+1-555-0102,USA,2024-01-16
1003,Bob,Johnson,bob.j@example.com,+1-555-0103,Canada,2024-01-17
1004,Alice,Williams,alice.w@example.com,+44-20-1234,UK,2024-01-18
1005,John,Doe,john.doe@example.com,+1-555-0101,USA,2024-01-15
"""

    print_info(f"Uploading sample customer CSV ({len(csv_content)} bytes)")

    # Upload via File Library API
    files = {
        'file': ('customers.csv', csv_content, 'text/csv')
    }
    metadata = {
        'source': 'workflow_demo',
        'description': 'Sample customer data for end-to-end workflow demo',
        'content_type': 'text/csv'
    }

    try:
        response = requests.post(
            f"{API_BASE}/file-library/upload",
            files=files,
            data={'metadata': json.dumps(metadata)}
        )
        response.raise_for_status()

        result = response.json()
        file_id = result['file_id']

        print_success(f"CSV uploaded successfully")
        print_json(result, "Upload Response")

        return file_id

    except requests.exceptions.RequestException as e:
        print_error(f"Failed to upload CSV: {e}")
        sys.exit(1)

# ============================================================================
# Step 2: Create Ontology Mapping
# ============================================================================

def step2_create_ontology_mapping() -> str:
    """Create ontology mapping for customer data"""
    print_header("STEP 2: Create Ontology Mapping for Customer Data")

    ontology_mapping = {
        "name": "CustomerOntology",
        "description": "Customer data ontology with standard fields",
        "version": "1.0",
        "classes": [
            {
                "name": "Customer",
                "uri": "http://graphica.io/ontology/Customer",
                "properties": [
                    {"name": "customerId", "type": "string", "required": True},
                    {"name": "firstName", "type": "string", "required": True},
                    {"name": "lastName", "type": "string", "required": True},
                    {"name": "email", "type": "string", "required": True},
                    {"name": "phone", "type": "string", "required": False},
                    {"name": "country", "type": "string", "required": False},
                    {"name": "registrationDate", "type": "date", "required": True}
                ]
            }
        ],
        "mappings": {
            "customer_id": "customerId",
            "first_name": "firstName",
            "last_name": "lastName",
            "email": "email",
            "phone": "phone",
            "country": "country",
            "registration_date": "registrationDate"
        }
    }

    print_info("Creating ontology mapping for Customer entity")
    print_json(ontology_mapping, "Ontology Definition")

    try:
        response = requests.post(
            f"{API_BASE}/ontology/mappings",
            json=ontology_mapping,
            headers={'Content-Type': 'application/json'}
        )
        response.raise_for_status()

        result = response.json()
        mapping_id = result.get('mapping_id', 'customer-ontology-v1')

        print_success(f"Ontology mapping created: {mapping_id}")
        return mapping_id

    except requests.exceptions.RequestException as e:
        print_warning(f"Ontology mapping creation skipped (endpoint may not be available): {e}")
        return "customer-ontology-v1"

# ============================================================================
# Step 3: Create Workflow Definition
# ============================================================================

def step3_create_workflow(file_id: str, ontology_mapping_id: str) -> str:
    """Create workflow with full pipeline"""
    print_header("STEP 3: Create Workflow Definition with Complete Pipeline")

    workflow_definition = {
        "name": "CustomerDataPipeline",
        "description": "End-to-end customer data processing with ontology alignment, transformation, deduplication, and DB2 loading",
        "version": "1.0",
        "routes": [
            {
                "id": "main_route",
                "name": "Customer Data Processing Route",
                "condition": {
                    "type": "always",
                    "expression": "true"
                },
                "actions": [
                    # Action 1: Parse CSV
                    {
                        "type": "transform",
                        "name": "parse_csv",
                        "description": "Parse uploaded CSV file",
                        "config": {
                            "transformer": "csv_parse",
                            "file_id": file_id,
                            "delimiter": ",",
                            "has_header": True,
                            "skip_empty_rows": True
                        }
                    },
                    # Action 2: Map to Ontology
                    {
                        "type": "transform",
                        "name": "map_to_ontology",
                        "description": "Map CSV fields to ontological classes",
                        "config": {
                            "transformer": "ontology_mapper",
                            "mapping_id": ontology_mapping_id,
                            "target_class": "Customer",
                            "validate_schema": True
                        }
                    },
                    # Action 3: Data Quality Validation
                    {
                        "type": "validate",
                        "name": "validate_customer_data",
                        "description": "Validate customer data quality",
                        "config": {
                            "rules": [
                                {
                                    "field": "email",
                                    "rule": "email_format",
                                    "error_level": "warning"
                                },
                                {
                                    "field": "customerId",
                                    "rule": "not_null",
                                    "error_level": "error"
                                }
                            ]
                        }
                    },
                    # Action 4: Deduplicate
                    {
                        "type": "transform",
                        "name": "deduplicate_customers",
                        "description": "Remove duplicate customer records",
                        "config": {
                            "transformer": "deduplicator",
                            "match_fields": ["email", "customerId"],
                            "strategy": "keep_first",
                            "similarity_threshold": 0.95
                        }
                    },
                    # Action 5: Enrich with Metadata
                    {
                        "type": "transform",
                        "name": "enrich_metadata",
                        "description": "Add processing metadata and lineage info",
                        "config": {
                            "transformer": "metadata_enricher",
                            "add_fields": {
                                "processed_at": "{{timestamp}}",
                                "workflow_id": "{{workflow_id}}",
                                "source_file": file_id,
                                "pipeline_version": "1.0"
                            }
                        }
                    },
                    # Action 6: Migrate to DB2
                    {
                        "type": "transform",
                        "name": "migrate_to_db2",
                        "description": "Load processed data into DB2",
                        "config": {
                            "transformer": "db2_migrate",
                            "connection": {
                                "host": "graphica-db2",
                                "port": 50000,
                                "database": "GRAPHICA",
                                "username": "db2inst1",
                                "password": "graphica-db2-pass"
                            },
                            "target_table": "CUSTOMERS",
                            "schema": "GRAPHICA_DATA",
                            "mode": "insert",
                            "create_table_if_not_exists": True
                        }
                    },
                    # Action 7: Capture Lineage
                    {
                        "type": "log",
                        "name": "capture_lineage",
                        "description": "Record complete data lineage",
                        "config": {
                            "log_level": "info",
                            "capture_lineage": True,
                            "lineage_graph": True
                        }
                    }
                ]
            }
        ],
        "metadata": {
            "author": "Graphica Demo",
            "tags": ["customer", "etl", "db2", "ontology"],
            "category": "data_integration"
        }
    }

    print_info("Creating comprehensive data processing workflow")
    print_json(workflow_definition, "Workflow Definition")

    try:
        response = requests.post(
            f"{WORKFLOW_API_BASE}/workflows",
            json=workflow_definition,
            headers={'Content-Type': 'application/json'}
        )
        response.raise_for_status()

        result = response.json()
        workflow_id = result['id']

        print_success(f"Workflow created successfully: {workflow_id}")
        print_json(result, "Created Workflow")

        return workflow_id

    except requests.exceptions.RequestException as e:
        print_error(f"Failed to create workflow: {e}")
        if hasattr(e, 'response') and e.response is not None:
            print_error(f"Response: {e.response.text}")
        sys.exit(1)

# ============================================================================
# Step 4: Execute Workflow (Async)
# ============================================================================

def step4_execute_workflow(workflow_id: str) -> str:
    """Execute workflow asynchronously"""
    print_header("STEP 4: Execute Workflow (Async)")

    execution_request = {
        "input": {
            "source": "file_library",
            "metadata": {
                "execution_mode": "async",
                "enable_lineage": True,
                "enable_profiling": True
            }
        }
    }

    print_info(f"Executing workflow: {workflow_id}")

    try:
        response = requests.post(
            f"{WORKFLOW_API_BASE}/workflows/{workflow_id}/execute-async",
            json=execution_request,
            headers={'Content-Type': 'application/json'}
        )
        response.raise_for_status()

        result = response.json()
        execution_id = result['execution_id']

        print_success(f"Workflow execution started: {execution_id}")
        print_json(result, "Execution Response")

        return execution_id

    except requests.exceptions.RequestException as e:
        print_error(f"Failed to execute workflow: {e}")
        if hasattr(e, 'response') and e.response is not None:
            print_error(f"Response: {e.response.text}")
        sys.exit(1)

# ============================================================================
# Step 5: Poll Execution Status
# ============================================================================

def step5_poll_execution_status(execution_id: str, max_attempts: int = 30) -> Dict[str, Any]:
    """Poll execution status until completion"""
    print_header("STEP 5: Monitor Workflow Execution")

    print_info(f"Polling execution status: {execution_id}")

    for attempt in range(max_attempts):
        try:
            response = requests.get(
                f"{WORKFLOW_API_BASE}/executions/{execution_id}",
                headers={'Accept': 'application/json'}
            )
            response.raise_for_status()

            result = response.json()
            status = result.get('status', 'unknown')

            print(f"  Attempt {attempt + 1}/{max_attempts}: Status = {status}")

            if status in ['completed', 'success']:
                print_success(f"Workflow execution completed successfully")
                print_json(result, "Final Execution Status")
                return result
            elif status in ['failed', 'error']:
                print_error(f"Workflow execution failed")
                print_json(result, "Failure Details")
                sys.exit(1)

            time.sleep(2)

        except requests.exceptions.RequestException as e:
            print_warning(f"Failed to get execution status (attempt {attempt + 1}): {e}")
            time.sleep(2)

    print_warning(f"Execution status polling timed out after {max_attempts} attempts")
    return {"status": "timeout"}

# ============================================================================
# Step 6: Retrieve Execution Logs
# ============================================================================

def step6_retrieve_execution_logs(execution_id: str):
    """Retrieve detailed execution logs"""
    print_header("STEP 6: Retrieve Execution Logs")

    print_info(f"Fetching logs for execution: {execution_id}")

    try:
        response = requests.get(
            f"{WORKFLOW_API_BASE}/executions/{execution_id}/logs",
            headers={'Accept': 'application/json'}
        )
        response.raise_for_status()

        logs = response.json()

        print_success(f"Retrieved {len(logs.get('entries', []))} log entries")
        print_json(logs, "Execution Logs")

        # Print action-by-action breakdown
        if 'entries' in logs:
            print("\n" + Colors.OKBLUE + "Action Execution Summary:" + Colors.ENDC)
            for entry in logs['entries']:
                action = entry.get('action', 'unknown')
                status = entry.get('status', 'unknown')
                duration = entry.get('duration_ms', 0)

                status_icon = "✓" if status == "success" else "✗"
                print(f"  {status_icon} {action}: {status} ({duration}ms)")

    except requests.exceptions.RequestException as e:
        print_warning(f"Failed to retrieve logs: {e}")

# ============================================================================
# Step 7: Query Lineage Graph
# ============================================================================

def step7_query_lineage(execution_id: str):
    """Query and display lineage information"""
    print_header("STEP 7: Query Data Lineage")

    print_info(f"Querying lineage for execution: {execution_id}")

    try:
        response = requests.get(
            f"{API_BASE}/lineage/execution/{execution_id}",
            headers={'Accept': 'application/json'}
        )
        response.raise_for_status()

        lineage = response.json()

        print_success("Lineage graph retrieved successfully")
        print_json(lineage, "Lineage Graph")

        # Display lineage summary
        if 'nodes' in lineage and 'edges' in lineage:
            node_count = len(lineage['nodes'])
            edge_count = len(lineage['edges'])

            print(f"\n{Colors.OKBLUE}Lineage Summary:{Colors.ENDC}")
            print(f"  • Nodes (data transformations): {node_count}")
            print(f"  • Edges (data flow): {edge_count}")

            # List transformations
            print(f"\n{Colors.OKBLUE}Transformation Chain:{Colors.ENDC}")
            for i, node in enumerate(lineage['nodes'], 1):
                node_type = node.get('type', 'unknown')
                node_name = node.get('name', 'unnamed')
                print(f"  {i}. {node_name} ({node_type})")

    except requests.exceptions.RequestException as e:
        print_warning(f"Failed to retrieve lineage: {e}")

# ============================================================================
# Step 8: Verify DB2 Data
# ============================================================================

def step8_verify_db2_data():
    """Verify data loaded into DB2"""
    print_header("STEP 8: Verify DB2 Data Load")

    print_info("Querying DB2 for loaded customer records")

    query_request = {
        "query": "SELECT COUNT(*) as record_count FROM GRAPHICA_DATA.CUSTOMERS",
        "connection": {
            "host": "graphica-db2",
            "port": 50000,
            "database": "GRAPHICA",
            "username": "db2inst1",
            "password": "graphica-db2-pass"
        }
    }

    try:
        response = requests.post(
            f"{API_BASE}/db2/query",
            json=query_request,
            headers={'Content-Type': 'application/json'}
        )
        response.raise_for_status()

        result = response.json()
        record_count = result.get('rows', [[0]])[0][0]

        print_success(f"DB2 verification successful: {record_count} records loaded")
        print_json(result, "DB2 Query Result")

    except requests.exceptions.RequestException as e:
        print_warning(f"DB2 verification skipped (endpoint may not be available): {e}")

# ============================================================================
# Main Execution Flow
# ============================================================================

def main():
    """Execute complete end-to-end workflow demo"""
    print_header("🚀 Graphica End-to-End Workflow API Demo 🚀")

    print(f"""
This demo showcases Graphica's complete data processing pipeline:

  1. {Colors.OKCYAN}Upload CSV data{Colors.ENDC} via File Library API
  2. {Colors.OKCYAN}Create ontology mapping{Colors.ENDC} for semantic alignment
  3. {Colors.OKCYAN}Define workflow{Colors.ENDC} with 7-step pipeline
  4. {Colors.OKCYAN}Execute workflow{Colors.ENDC} asynchronously
  5. {Colors.OKCYAN}Monitor execution{Colors.ENDC} until completion
  6. {Colors.OKCYAN}Retrieve logs{Colors.ENDC} for audit trail
  7. {Colors.OKCYAN}Query lineage{Colors.ENDC} for full provenance
  8. {Colors.OKCYAN}Verify DB2 load{Colors.ENDC} for data persistence

Prerequisites:
  • Graphica coordinator running at {COORDINATOR_BASE_URL}
  • DB2 instance configured and accessible
""")

    input("Press Enter to begin the demo...")

    try:
        # Step 1: Upload CSV
        file_id = step1_upload_csv_data()

        # Step 2: Create Ontology Mapping
        ontology_mapping_id = step2_create_ontology_mapping()

        # Step 3: Create Workflow
        workflow_id = step3_create_workflow(file_id, ontology_mapping_id)

        # Step 4: Execute Workflow
        execution_id = step4_execute_workflow(workflow_id)

        # Step 5: Poll Execution Status
        execution_result = step5_poll_execution_status(execution_id)

        # Step 6: Retrieve Logs
        step6_retrieve_execution_logs(execution_id)

        # Step 7: Query Lineage
        step7_query_lineage(execution_id)

        # Step 8: Verify DB2 Data
        step8_verify_db2_data()

        # Final Summary
        print_header("✓ Demo Completed Successfully!")
        print(f"""
{Colors.OKGREEN}Summary:{Colors.ENDC}
  • File ID: {file_id}
  • Ontology Mapping: {ontology_mapping_id}
  • Workflow ID: {workflow_id}
  • Execution ID: {execution_id}
  • Status: {execution_result.get('status', 'unknown')}

{Colors.OKCYAN}What was demonstrated:{Colors.ENDC}
  ✓ CSV file upload and storage
  ✓ Ontology-based data modeling
  ✓ Multi-step data transformation pipeline
  ✓ Data quality validation
  ✓ Deduplication of records
  ✓ Complete lineage tracking
  ✓ DB2 database migration
  ✓ Async workflow execution
  ✓ Real-time progress monitoring

{Colors.BOLD}Next Steps:{Colors.ENDC}
  • Explore workflow variants with different transformers
  • Try streaming workflows for real-time processing
  • Integrate ML model predictions
  • Scale to production workloads
        """)

    except KeyboardInterrupt:
        print_warning("\n\nDemo interrupted by user")
        sys.exit(1)
    except Exception as e:
        print_error(f"\n\nDemo failed with unexpected error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)

if __name__ == "__main__":
    main()
