#!/usr/bin/env python3
"""
Healthcare ETL Demo v5 - Enhanced with SPARQL Validation

New features in v5:
- SPARQL queries to validate RDF governance data
- Workflow execution metadata validation
- Data quality metrics
- Semantic consistency checks
- Provenance chain verification
"""

import csv
import json
import os
import random
import sys
from datetime import datetime, timedelta
from pathlib import Path
from typing import Dict, Any, List, Optional

from graphica import Client
from graphica.errors import NotFoundError

# Configuration
SERVER_URL = os.getenv("GRAPHICA_URL", "http://localhost:8080")
OUTPUT_CSV = "/tmp/healthcare_deduped_1m.csv"
INPUT_CSV = "/tmp/healthcare_patients_1m.csv"

# Import the data generation from v4
sys.path.insert(0, os.path.dirname(__file__))

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


def validate_provenance_chain(client: Client, sample_size: int = 5) -> None:
    """Validate end-to-end provenance chain for sample records."""
    print("\n" + "=" * 60)
    print("Provenance Chain Validation")
    print("=" * 60)
    
    # Get random sample records
    with open(OUTPUT_CSV, 'r') as f:
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


# Note: This is a template - you'll need to integrate with healthcare_etl_demo_v4.py
# for the full workflow execution and data generation functions

if __name__ == "__main__":
    print("Healthcare ETL Demo v5 - Enhanced Validation")
    print("=" * 60)
    print("\nThis script provides enhanced validation functions.")
    print("Integrate with v4 script for full demo execution.")
    print("\nNew v5 features:")
    print("  • SPARQL governance queries")
    print("  • Workflow metadata validation")  
    print("  • Data quality metrics")
    print("  • Semantic consistency checks")
    print("  • Provenance chain validation")
    print("  • JSON validation report")
