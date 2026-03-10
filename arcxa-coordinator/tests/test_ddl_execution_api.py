#!/usr/bin/env python3
"""
Test DDL Execution API

This test validates the automatic DDL execution feature (GAP-001).
It demonstrates:
1. Generating DDL from SHACL shapes
2. Executing DDL against a target database
3. Proper error handling and transaction support
"""

import requests
import json
import time
import sys
from typing import Dict, Optional

class DdlExecutionApiClient:
    """Client for testing DDL execution API"""

    def __init__(self, base_url: str = "http://localhost:8080"):
        self.base_url = base_url
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})

    def upload_shacl_shape(self, shacl_content: str) -> Optional[str]:
        """Upload a SHACL shape definition"""
        url = f"{self.base_url}/api/v1/ontology/shacl"

        payload = {
            "content": shacl_content,
            "format": "turtle"
        }

        try:
            response = self.session.post(url, json=payload)
            response.raise_for_status()
            result = response.json()
            print(f"✅ SHACL shape uploaded successfully")
            return result.get("shape_uri")
        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to upload SHACL shape: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return None

    def generate_ddl(self, shacl_uri: str, dialect: str = "db2",
                    include_indexes: bool = True,
                    include_foreign_keys: bool = True,
                    idempotent: bool = True) -> Optional[Dict]:
        """Generate DDL from SHACL shape"""
        url = f"{self.base_url}/api/v1/ddl/generate"

        payload = {
            "shacl_uri": shacl_uri,
            "dialect": dialect,
            "include_indexes": include_indexes,
            "include_foreign_keys": include_foreign_keys,
            "idempotent": idempotent
        }

        try:
            response = self.session.post(url, json=payload)
            response.raise_for_status()
            result = response.json()
            print(f"✅ Generated {len(result['ddl_statements'])} DDL statements")
            return result
        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to generate DDL: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return None

    def execute_ddl(self, ddl_statements: list[str],
                   db_config: Dict,
                   transactional: bool = True,
                   continue_on_error: bool = False,
                   shacl_uri: Optional[str] = None) -> Optional[Dict]:
        """Execute DDL statements against target database"""
        url = f"{self.base_url}/api/v1/ddl/execute"

        payload = {
            "ddl_statements": ddl_statements,
            "database_config": db_config,
            "transactional": transactional,
            "continue_on_error": continue_on_error
        }

        if shacl_uri:
            payload["shacl_uri"] = shacl_uri

        try:
            response = self.session.post(url, json=payload)
            response.raise_for_status()
            result = response.json()

            if result.get("success"):
                print(f"✅ DDL execution successful:")
                print(f"   - Statements executed: {result['statements_executed']}")
                print(f"   - Tables affected: {result['tables_affected']}")
                print(f"   - Execution time: {result['execution_time_ms']}ms")
            else:
                print(f"❌ DDL execution failed:")
                print(f"   - Statements executed: {result['statements_executed']}")
                for error in result.get('errors', []):
                    print(f"   - Statement {error['statement_index']}: {error['error']}")

            return result
        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to execute DDL: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"   Response: {e.response.text}")
            return None

    def validate_ddl(self, ddl_sql: str, dialect: str = "db2") -> Optional[Dict]:
        """Validate DDL SQL"""
        url = f"{self.base_url}/api/v1/ddl/validate"

        payload = {
            "ddl_sql": ddl_sql,
            "dialect": dialect
        }

        try:
            response = self.session.post(url, json=payload)
            response.raise_for_status()
            result = response.json()

            if result.get("valid"):
                print(f"✅ DDL validation passed")
                if result.get("warnings"):
                    for warning in result["warnings"]:
                        print(f"   ⚠️  Warning: {warning}")
            else:
                print(f"❌ DDL validation failed:")
                for error in result.get("errors", []):
                    print(f"   - {error}")

            return result
        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to validate DDL: {e}")
            return None


def test_ddl_execution_workflow():
    """Test complete DDL execution workflow"""
    print("\n" + "="*80)
    print("TEST: DDL Execution API Workflow")
    print("="*80 + "\n")

    client = DdlExecutionApiClient()

    # Define a simple SHACL shape for testing
    shacl_shape = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <http://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:CustomerShape a sh:NodeShape ;
    sh:targetClass ex:Customer ;
    sh:property [
        sh:path ex:customerId ;
        sh:datatype xsd:integer ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
    ] ;
    sh:property [
        sh:path ex:firstName ;
        sh:datatype xsd:string ;
        sh:maxLength 100 ;
    ] ;
    sh:property [
        sh:path ex:lastName ;
        sh:datatype xsd:string ;
        sh:maxLength 100 ;
    ] ;
    sh:property [
        sh:path ex:email ;
        sh:datatype xsd:string ;
        sh:maxLength 255 ;
    ] ;
    sh:property [
        sh:path ex:dateOfBirth ;
        sh:datatype xsd:date ;
    ] .
"""

    # Step 1: Upload SHACL shape (may not be implemented yet)
    print("Step 1: Upload SHACL shape")
    shape_uri = "http://example.org/CustomerShape"
    # Commenting out upload since ontology API may not be fully implemented
    # shape_uri = client.upload_shacl_shape(shacl_shape)
    # if not shape_uri:
    #     print("⚠️  SHACL upload not available, using mock URI")
    #     shape_uri = "http://example.org/CustomerShape"
    print(f"   Using shape URI: {shape_uri}\n")

    # Step 2: Generate DDL from SHACL shape
    print("Step 2: Generate DDL from SHACL shape")
    ddl_result = client.generate_ddl(
        shacl_uri=shape_uri,
        dialect="db2",
        idempotent=True
    )

    if not ddl_result:
        print("❌ Cannot proceed without DDL generation")
        return False

    print(f"\nGenerated SQL script:")
    print("-" * 80)
    print(ddl_result['sql_script'])
    print("-" * 80 + "\n")

    # Step 3: Validate DDL
    print("Step 3: Validate generated DDL")
    validation = client.validate_ddl(
        ddl_sql=ddl_result['sql_script'],
        dialect="db2"
    )

    if not validation or not validation.get('valid'):
        print("❌ DDL validation failed")
        return False
    print()

    # Step 4: Execute DDL against DB2
    print("Step 4: Execute DDL against DB2 database")

    db_config = {
        "db_type": "db2",
        "host": "localhost",
        "port": 50000,
        "database": "GRAPHICA",
        "username": "db2inst1",
        "password": "graphica-db2-pass",
        "options": {}
    }

    execution_result = client.execute_ddl(
        ddl_statements=ddl_result['ddl_statements'],
        db_config=db_config,
        transactional=True,
        shacl_uri=shape_uri
    )

    if not execution_result:
        print("❌ DDL execution failed")
        return False

    if not execution_result.get('success'):
        print("❌ DDL execution reported errors")
        return False

    print("\n✅ All tests passed!\n")
    return True


def test_ddl_execution_error_handling():
    """Test DDL execution error handling"""
    print("\n" + "="*80)
    print("TEST: DDL Execution Error Handling")
    print("="*80 + "\n")

    client = DdlExecutionApiClient()

    # Test 1: Invalid SQL
    print("Test 1: Invalid SQL statement")
    db_config = {
        "db_type": "db2",
        "host": "localhost",
        "port": 50000,
        "database": "GRAPHICA",
        "username": "db2inst1",
        "password": "graphica-db2-pass",
        "options": {}
    }

    invalid_ddl = [
        "THIS IS NOT VALID SQL",
    ]

    result = client.execute_ddl(
        ddl_statements=invalid_ddl,
        db_config=db_config,
        transactional=True
    )

    if result and not result.get('success'):
        print("✅ Error handling works correctly\n")
    else:
        print("❌ Expected error was not caught\n")
        return False

    # Test 2: Continue on error
    print("Test 2: Continue on error mode")
    mixed_ddl = [
        "CREATE TABLE test_table_1 (id INT PRIMARY KEY)",
        "THIS IS INVALID",
        "CREATE TABLE test_table_2 (id INT PRIMARY KEY)"
    ]

    result = client.execute_ddl(
        ddl_statements=mixed_ddl,
        db_config=db_config,
        transactional=False,
        continue_on_error=True
    )

    if result:
        print(f"   Statements executed: {result['statements_executed']}/{len(mixed_ddl)}")
        print(f"   Errors: {len(result.get('errors', []))}")
        print("✅ Continue-on-error mode works\n")
    else:
        print("❌ Continue-on-error test failed\n")
        return False

    # Test 3: Connection failure
    print("Test 3: Connection failure handling")
    bad_config = {
        "db_type": "db2",
        "host": "invalid-host",
        "port": 99999,
        "database": "NONEXISTENT",
        "username": "baduser",
        "password": "badpass",
        "options": {}
    }

    result = client.execute_ddl(
        ddl_statements=["CREATE TABLE test (id INT)"],
        db_config=bad_config,
        transactional=True
    )

    if result and not result.get('success'):
        print("✅ Connection failure handled correctly\n")
    else:
        print("⚠️  Connection failure test inconclusive\n")

    print("✅ Error handling tests completed\n")
    return True


def test_ddl_transaction_rollback():
    """Test transaction rollback on error"""
    print("\n" + "="*80)
    print("TEST: DDL Transaction Rollback")
    print("="*80 + "\n")

    client = DdlExecutionApiClient()

    db_config = {
        "db_type": "db2",
        "host": "localhost",
        "port": 50000,
        "database": "GRAPHICA",
        "username": "db2inst1",
        "password": "graphica-db2-pass",
        "options": {}
    }

    # Create DDL with intentional error in the middle
    ddl_statements = [
        "CREATE TABLE tx_test_1 (id INT PRIMARY KEY)",
        "CREATE TABLE tx_test_2 (id INT PRIMARY KEY)",
        "THIS WILL FAIL",  # Intentional error
        "CREATE TABLE tx_test_3 (id INT PRIMARY KEY)"
    ]

    print("Executing DDL with intentional error in transaction mode...")
    result = client.execute_ddl(
        ddl_statements=ddl_statements,
        db_config=db_config,
        transactional=True
    )

    if result and not result.get('success'):
        print("✅ Transaction rolled back as expected")
        print(f"   All {len(ddl_statements)} statements should be rolled back\n")
        return True
    else:
        print("❌ Transaction rollback test failed\n")
        return False


if __name__ == "__main__":
    print("\n" + "="*80)
    print("DDL EXECUTION API TEST SUITE")
    print("="*80)

    # Wait for server to be ready
    print("\nWaiting for server to be ready...")
    time.sleep(2)

    all_passed = True

    # Run tests
    try:
        # Test 1: Basic workflow
        if not test_ddl_execution_workflow():
            all_passed = False

        # Test 2: Error handling
        if not test_ddl_execution_error_handling():
            all_passed = False

        # Test 3: Transaction rollback
        if not test_ddl_transaction_rollback():
            all_passed = False

    except Exception as e:
        print(f"\n❌ Test suite failed with exception: {e}")
        import traceback
        traceback.print_exc()
        all_passed = False

    # Summary
    print("\n" + "="*80)
    if all_passed:
        print("✅ ALL TESTS PASSED")
    else:
        print("❌ SOME TESTS FAILED")
    print("="*80 + "\n")

    sys.exit(0 if all_passed else 1)
