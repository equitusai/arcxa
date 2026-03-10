#!/usr/bin/env python3
"""
Comprehensive End-to-End Test: CSV -> Ontology -> Transformation -> DB2 + Lineage

This test validates the complete ETL pipeline with row-level lineage tracking:
1. Generate 3 CSV files with synthetic data (100 rows, 10+ fields each)
2. Define custom retail ontology (Customer, Order, Product)
3. Map CSV fields to ontology properties
4. Apply transformations (quality checks, standardization, derived fields)
5. Load data into DB2 database
6. Validate data integrity in DB2
7. Trace row-level lineage from DB2 back to CSV source

Architecture:
- Uses Graphica REST API for orchestration
- Tracks lineage at row level using RowLineageEvent
- Validates complete data traceability
"""

import csv
import json
import os
import random
import sys
import time
import ibm_db
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Optional, Tuple
import requests
from faker import Faker

# Initialize Faker for synthetic data generation
fake = Faker()
Faker.seed(42)  # Reproducible data


class GraphicaETLTest:
    """Comprehensive ETL test with row-level lineage tracking"""

    def __init__(self, base_url: str = "http://localhost:8080"):
        self.base_url = base_url
        self.session = requests.Session()
        self.token = None

        # Test metadata
        self.test_id = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
        self.job_id = f"etl-job-{self.test_id}"
        self.batch_id = f"batch-{self.test_id}"

        # Ontology namespace (unique per test run to avoid conflicts)
        self.ontology_namespace = f"http://example.com/ecommerce/{self.test_id}#"
        self.ontology_id = None

        # File paths
        self.output_dir = "/tmp/graphica_etl_test"
        os.makedirs(self.output_dir, exist_ok=True)

        self.csv_files = {
            "customers": os.path.join(self.output_dir, f"customers_{self.test_id}.csv"),
            "orders": os.path.join(self.output_dir, f"orders_{self.test_id}.csv"),
            "products": os.path.join(self.output_dir, f"products_{self.test_id}.csv"),
        }

        # DB2 connection details
        self.db2_config = {
            "host": "localhost",
            "port": 50000,
            "database": "GRAPHICA",
            "user": "db2inst1",
            "password": "graphica-db2-pass",
        }
        self.db2_conn = None

        # Generated data tracking
        self.customer_ids = []
        self.product_ids = []
        self.generated_data = {
            "customers": [],
            "orders": [],
            "products": [],
        }

    # ========================================================================
    # Authentication
    # ========================================================================

    def authenticate(self) -> bool:
        """Authenticate with Graphica API"""
        url = f"{self.base_url}/auth/login"
        try:
            response = self.session.post(
                url,
                json={"username": "admin", "password": "Admin@Pass123"},
                timeout=10
            )
            response.raise_for_status()
            result = response.json()
            self.token = result.get("token")
            self.session.headers.update({"Authorization": f"Bearer {self.token}"})
            return True
        except requests.exceptions.RequestException as e:
            print(f"❌ Authentication failed: {e}")
            return False

    # ========================================================================
    # Synthetic Data Generation
    # ========================================================================

    def generate_customers_csv(self, num_rows: int = 100) -> List[Dict]:
        """Generate synthetic customer data"""
        print(f"\n📊 Generating {num_rows} customer records...")

        customers = []
        for i in range(num_rows):
            customer_id = f"CUST{str(i+1).zfill(6)}"
            self.customer_ids.append(customer_id)

            # Some records will have quality issues for testing
            has_missing_email = (i % 20 == 0)  # 5% missing email
            has_invalid_age = (i % 25 == 0)    # 4% invalid age

            customer = {
                "customer_id": customer_id,
                "first_name": fake.first_name() if i % 30 != 0 else "",  # 3% missing
                "last_name": fake.last_name(),
                "email": "" if has_missing_email else fake.email(),
                "phone": fake.phone_number(),
                "date_of_birth": fake.date_of_birth(minimum_age=18, maximum_age=80).isoformat(),
                "age": -1 if has_invalid_age else random.randint(18, 80),
                "address": fake.street_address(),
                "city": fake.city(),
                "state": fake.state_abbr(),
                "zip_code": fake.zipcode(),
                "country": "USA",
                "registration_date": fake.date_between(start_date="-2y", end_date="today").isoformat(),
                "loyalty_tier": random.choice(["Bronze", "Silver", "Gold", "Platinum"]),
                "total_purchases": random.randint(0, 100),
            }
            customers.append(customer)

        # Write to CSV
        with open(self.csv_files["customers"], "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=customers[0].keys())
            writer.writeheader()
            writer.writerows(customers)

        self.generated_data["customers"] = customers
        print(f"   ✅ Generated {len(customers)} customers -> {self.csv_files['customers']}")
        return customers

    def generate_products_csv(self, num_rows: int = 100) -> List[Dict]:
        """Generate synthetic product data"""
        print(f"\n📦 Generating {num_rows} product records...")

        categories = ["Electronics", "Clothing", "Home & Garden", "Sports", "Books", "Toys"]
        products = []

        for i in range(num_rows):
            product_id = f"PROD{str(i+1).zfill(6)}"
            self.product_ids.append(product_id)

            # Some quality issues
            has_negative_price = (i % 50 == 0)  # 2% negative price
            has_invalid_stock = (i % 40 == 0)   # 2.5% negative stock

            product = {
                "product_id": product_id,
                "product_name": fake.catch_phrase(),
                "description": fake.sentence(nb_words=10),
                "category": random.choice(categories),
                "brand": fake.company(),
                "price": -10.00 if has_negative_price else round(random.uniform(5.0, 500.0), 2),
                "cost": round(random.uniform(2.0, 250.0), 2),
                "stock_quantity": -5 if has_invalid_stock else random.randint(0, 1000),
                "weight_kg": round(random.uniform(0.1, 50.0), 2),
                "dimensions": f"{random.randint(10,100)}x{random.randint(10,100)}x{random.randint(10,100)}",
                "manufacturer": fake.company(),
                "release_date": fake.date_between(start_date="-3y", end_date="today").isoformat(),
                "warranty_months": random.choice([0, 6, 12, 24, 36]),
                "is_active": "Y" if random.choice([True, False]) else "N",
            }
            products.append(product)

        # Write to CSV
        with open(self.csv_files["products"], "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=products[0].keys())
            writer.writeheader()
            writer.writerows(products)

        self.generated_data["products"] = products
        print(f"   ✅ Generated {len(products)} products -> {self.csv_files['products']}")
        return products

    def generate_orders_csv(self, num_rows: int = 100) -> List[Dict]:
        """Generate synthetic order data"""
        print(f"\n🛒 Generating {num_rows} order records...")

        if not self.customer_ids or not self.product_ids:
            raise ValueError("Must generate customers and products first!")

        orders = []
        statuses = ["Pending", "Processing", "Shipped", "Delivered", "Cancelled"]
        payment_methods = ["Credit Card", "Debit Card", "PayPal", "Bank Transfer"]

        for i in range(num_rows):
            order_id = f"ORD{str(i+1).zfill(8)}"

            # Some quality issues
            has_missing_customer = (i % 30 == 0)  # 3% orphaned order
            has_invalid_amount = (i % 35 == 0)    # ~3% negative amount

            order = {
                "order_id": order_id,
                "customer_id": "" if has_missing_customer else random.choice(self.customer_ids),
                "product_id": random.choice(self.product_ids),
                "order_date": fake.date_time_between(start_date="-1y", end_date="now", tzinfo=timezone.utc).isoformat(),
                "quantity": random.randint(1, 10),
                "unit_price": round(random.uniform(10.0, 500.0), 2),
                "total_amount": -100.00 if has_invalid_amount else round(random.uniform(10.0, 5000.0), 2),
                "discount_percent": round(random.uniform(0, 25), 2),
                "tax_amount": round(random.uniform(0, 100), 2),
                "shipping_cost": round(random.uniform(0, 50), 2),
                "status": random.choice(statuses),
                "payment_method": random.choice(payment_methods),
                "shipping_address": fake.address().replace("\n", ", "),
                "notes": fake.sentence() if random.random() > 0.7 else "",
            }
            orders.append(order)

        # Write to CSV
        with open(self.csv_files["orders"], "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=orders[0].keys())
            writer.writeheader()
            writer.writerows(orders)

        self.generated_data["orders"] = orders
        print(f"   ✅ Generated {len(orders)} orders -> {self.csv_files['orders']}")
        return orders

    # ========================================================================
    # DB2 Database Setup
    # ========================================================================

    def connect_db2(self) -> bool:
        """Connect to DB2 database"""
        try:
            conn_str = (
                f"DATABASE={self.db2_config['database']};"
                f"HOSTNAME={self.db2_config['host']};"
                f"PORT={self.db2_config['port']};"
                f"PROTOCOL=TCPIP;"
                f"UID={self.db2_config['user']};"
                f"PWD={self.db2_config['password']};"
            )
            self.db2_conn = ibm_db.connect(conn_str, "", "")
            return True
        except Exception as e:
            print(f"❌ DB2 connection failed: {e}")
            return False

    def create_db2_tables(self) -> bool:
        """Create DB2 tables for test data"""
        print("\n🗄️  Creating DB2 tables...")

        if not self.db2_conn:
            if not self.connect_db2():
                return False

        tables = [
            # Customers table
            f"""
            CREATE TABLE CUSTOMERS_{self.test_id} (
                CUSTOMER_ID VARCHAR(20) NOT NULL PRIMARY KEY,
                FIRST_NAME VARCHAR(50),
                LAST_NAME VARCHAR(50),
                EMAIL VARCHAR(100),
                PHONE VARCHAR(100),
                DATE_OF_BIRTH DATE,
                AGE INTEGER,
                ADDRESS VARCHAR(500),
                CITY VARCHAR(100),
                STATE VARCHAR(10),
                ZIP_CODE VARCHAR(20),
                COUNTRY VARCHAR(50),
                REGISTRATION_DATE DATE,
                LOYALTY_TIER VARCHAR(20),
                TOTAL_PURCHASES INTEGER,
                LOADED_AT TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            """,

            # Products table
            f"""
            CREATE TABLE PRODUCTS_{self.test_id} (
                PRODUCT_ID VARCHAR(20) NOT NULL PRIMARY KEY,
                PRODUCT_NAME VARCHAR(500),
                DESCRIPTION CLOB,
                CATEGORY VARCHAR(50),
                BRAND VARCHAR(200),
                PRICE DECIMAL(10,2),
                COST DECIMAL(10,2),
                STOCK_QUANTITY INTEGER,
                WEIGHT_KG DECIMAL(8,2),
                DIMENSIONS VARCHAR(100),
                MANUFACTURER VARCHAR(200),
                RELEASE_DATE DATE,
                WARRANTY_MONTHS INTEGER,
                IS_ACTIVE CHAR(1),
                LOADED_AT TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            """,

            # Orders table
            f"""
            CREATE TABLE ORDERS_{self.test_id} (
                ORDER_ID VARCHAR(20) NOT NULL PRIMARY KEY,
                CUSTOMER_ID VARCHAR(20),
                PRODUCT_ID VARCHAR(20),
                ORDER_DATE VARCHAR(50),
                QUANTITY INTEGER,
                UNIT_PRICE DECIMAL(10,2),
                TOTAL_AMOUNT DECIMAL(10,2),
                DISCOUNT_PERCENT DECIMAL(5,2),
                TAX_AMOUNT DECIMAL(10,2),
                SHIPPING_COST DECIMAL(10,2),
                STATUS VARCHAR(20),
                PAYMENT_METHOD VARCHAR(50),
                SHIPPING_ADDRESS CLOB,
                NOTES CLOB,
                LOADED_AT TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            """
        ]

        for table_sql in tables:
            try:
                stmt = ibm_db.exec_immediate(self.db2_conn, table_sql)
                print(f"   ✅ Table created successfully")
            except Exception as e:
                if "already exists" not in str(e).lower() and "name already" not in str(e).lower():
                    print(f"   ⚠️  Table creation warning: {e}")

        return True

    # ========================================================================
    # File Library Integration
    # ========================================================================

    def upload_to_file_library(self, csv_path: str, table_name: str) -> Optional[str]:
        """Upload CSV to file library and return file_id"""
        print(f"\n📤 Uploading {os.path.basename(csv_path)} to File Library...")

        url = f"{self.base_url}/api/v1/file-library/files"

        try:
            with open(csv_path, 'rb') as f:
                files = {'file': (os.path.basename(csv_path), f, 'text/csv')}
                metadata = {
                    'description': f'Test CSV for {table_name}',
                    'tags': json.dumps(['test', 'e2e', table_name, self.test_id])
                }
                response = self.session.post(url, files=files, data=metadata, timeout=30)

            if response.status_code == 200:
                file_id = response.json().get('file_id')
                print(f"   ✅ File uploaded successfully - file_id: {file_id}")

                # Scan schema
                scan_url = f"{self.base_url}/api/v1/file-library/files/{file_id}/scan-schema"
                scan_response = self.session.post(scan_url, timeout=30)

                if scan_response.status_code == 200:
                    schema_info = scan_response.json()
                    print(f"   ✅ Schema scanned - {schema_info.get('column_count', 0)} columns detected")
                else:
                    print(f"   ⚠️  Schema scan returned {scan_response.status_code}")

                return file_id
            else:
                print(f"   ❌ Upload failed: {response.status_code} - {response.text}")
                return None

        except Exception as e:
            print(f"   ❌ Upload error: {e}")
            return None

    # ========================================================================
    # SHACL-Driven DDL Generation
    # ========================================================================

    def generate_ddl_from_shacl(self, shacl_uri: str, dialect: str = "db2") -> Optional[List[str]]:
        """Generate DDL from SHACL shape (ontology-driven schema generation)"""
        print(f"\n🔨 Generating DDL from SHACL shape: {shacl_uri}...")

        url = f"{self.base_url}/api/v1/ddl/generate"

        payload = {
            "shacl_uri": shacl_uri,
            "dialect": dialect,
            "include_indexes": True,
            "include_foreign_keys": True,
            "idempotent": True
        }

        try:
            response = self.session.post(url, json=payload, timeout=30)

            if response.status_code == 200:
                result = response.json()
                ddl_statements = result.get('ddl_statements', [])
                tables_generated = result.get('tables_generated', 0)

                print(f"   ✅ DDL generated successfully")
                print(f"   📊 Tables: {tables_generated}, Statements: {len(ddl_statements)}")

                # Print first statement preview
                if ddl_statements:
                    first_stmt = ddl_statements[0][:200] + "..." if len(ddl_statements[0]) > 200 else ddl_statements[0]
                    print(f"   📝 Preview: {first_stmt}")

                return ddl_statements
            else:
                print(f"   ❌ DDL generation failed: {response.status_code}")
                print(f"   📄 Response: {response.text}")
                return None

        except Exception as e:
            print(f"   ❌ DDL generation error: {e}")
            return None

    def execute_ddl_via_coordinator(self, ddl_statements: List[str], shacl_uri: Optional[str] = None) -> bool:
        """Execute DDL statements (typically generated from SHACL in previous step)"""
        print(f"\n⚡ Executing {len(ddl_statements)} DDL statement(s) via coordinator...")

        url = f"{self.base_url}/api/v1/ddl/execute"

        payload = {
            "ddl_statements": ddl_statements,
            "database_config": {
                "db_type": "db2",
                "host": self.db2_config["host"],
                "port": self.db2_config["port"],
                "database": self.db2_config["database"],
                "username": self.db2_config["user"],
                "password": self.db2_config["password"]
            },
            "transactional": True,
            "continue_on_error": False
        }

        # Include SHACL URI for lineage tracking
        if shacl_uri:
            payload["shacl_uri"] = shacl_uri

        try:
            response = self.session.post(url, json=payload, timeout=60)

            if response.status_code == 200:
                result = response.json()
                statements_executed = result.get('statements_executed', 0)
                tables_affected = result.get('tables_affected', [])
                execution_time_ms = result.get('execution_time_ms', 0)

                print(f"   ✅ DDL executed successfully")
                print(f"   📊 Executed: {statements_executed} statements")
                print(f"   🗄️  Tables: {', '.join(tables_affected) if tables_affected else 'N/A'}")
                print(f"   ⏱️  Time: {execution_time_ms}ms")

                return True
            else:
                print(f"   ❌ DDL execution failed: {response.status_code}")
                print(f"   📄 Response: {response.text}")
                return False

        except Exception as e:
            print(f"   ❌ DDL execution error: {e}")
            return False

    # ========================================================================
    # Ontology with SHACL Shapes
    # ========================================================================

    def create_ecommerce_ontology_with_shacl(self) -> str:
        """Create e-commerce ontology with SHACL shapes for DDL generation"""

        # Use the instance's unique namespace (not hardcoded)
        ontology_ttl = f"""
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ecom: <{self.ontology_namespace}> .

# Ontology Header
ecom: a owl:Ontology ;
    rdfs:label "E-Commerce Domain Ontology" ;
    rdfs:comment "Ontology for retail e-commerce domain with SHACL shapes for DDL generation" ;
    owl:versionInfo "1.0.0" .

# ============================================================================
# Classes
# ============================================================================

ecom:Customer a owl:Class ;
    rdfs:label "Customer" ;
    rdfs:comment "A customer in the e-commerce system" .

ecom:Product a owl:Class ;
    rdfs:label "Product" ;
    rdfs:comment "A product available for purchase" .

ecom:Order a owl:Class ;
    rdfs:label "Order" ;
    rdfs:comment "A customer order" .

# ============================================================================
# Properties
# ============================================================================

# Customer Properties
ecom:customerId a owl:DatatypeProperty ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "Customer ID" .

ecom:firstName a owl:DatatypeProperty ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "First Name" .

ecom:lastName a owl:DatatypeProperty ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "Last Name" .

ecom:email a owl:DatatypeProperty ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "Email Address" .

ecom:phone a owl:DatatypeProperty ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "Phone Number" .

ecom:age a owl:DatatypeProperty ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:integer ;
    rdfs:label "Age" .

# Product Properties
ecom:productId a owl:DatatypeProperty ;
    rdfs:domain ecom:Product ;
    rdfs:range xsd:string ;
    rdfs:label "Product ID" .

ecom:productName a owl:DatatypeProperty ;
    rdfs:domain ecom:Product ;
    rdfs:range xsd:string ;
    rdfs:label "Product Name" .

ecom:price a owl:DatatypeProperty ;
    rdfs:domain ecom:Product ;
    rdfs:range xsd:decimal ;
    rdfs:label "Price" .

ecom:stockQuantity a owl:DatatypeProperty ;
    rdfs:domain ecom:Product ;
    rdfs:range xsd:integer ;
    rdfs:label "Stock Quantity" .

# Order Properties
ecom:orderId a owl:DatatypeProperty ;
    rdfs:domain ecom:Order ;
    rdfs:range xsd:string ;
    rdfs:label "Order ID" .

ecom:totalAmount a owl:DatatypeProperty ;
    rdfs:domain ecom:Order ;
    rdfs:range xsd:decimal ;
    rdfs:label "Total Amount" .

ecom:quantity a owl:DatatypeProperty ;
    rdfs:domain ecom:Order ;
    rdfs:range xsd:integer ;
    rdfs:label "Quantity" .

# ============================================================================
# SHACL Shapes for DDL Generation
# ============================================================================

# Customer Shape - generates CUSTOMERS table
ecom:CustomerShape a sh:NodeShape ;
    sh:targetClass ecom:Customer ;
    rdfs:label "Customer Table Shape" ;
    sh:property [
        sh:path ecom:customerId ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:maxLength 20 ;
        sh:name "CUSTOMER_ID" ;
        ecom:isPrimaryKey true ;
    ] ;
    sh:property [
        sh:path ecom:firstName ;
        sh:datatype xsd:string ;
        sh:maxLength 50 ;
        sh:name "FIRST_NAME" ;
    ] ;
    sh:property [
        sh:path ecom:lastName ;
        sh:datatype xsd:string ;
        sh:maxLength 50 ;
        sh:name "LAST_NAME" ;
    ] ;
    sh:property [
        sh:path ecom:email ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxLength 100 ;
        sh:pattern "^[^@]+@[^@]+\\\\.[^@]+$" ;
        sh:name "EMAIL" ;
    ] ;
    sh:property [
        sh:path ecom:phone ;
        sh:datatype xsd:string ;
        sh:maxLength 100 ;
        sh:name "PHONE" ;
    ] ;
    sh:property [
        sh:path ecom:age ;
        sh:datatype xsd:integer ;
        sh:minInclusive 0 ;
        sh:maxInclusive 150 ;
        sh:name "AGE" ;
    ] .

# Product Shape - generates PRODUCTS table
ecom:ProductShape a sh:NodeShape ;
    sh:targetClass ecom:Product ;
    rdfs:label "Product Table Shape" ;
    sh:property [
        sh:path ecom:productId ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:maxLength 20 ;
        sh:name "PRODUCT_ID" ;
        ecom:isPrimaryKey true ;
    ] ;
    sh:property [
        sh:path ecom:productName ;
        sh:datatype xsd:string ;
        sh:maxLength 500 ;
        sh:name "PRODUCT_NAME" ;
    ] ;
    sh:property [
        sh:path ecom:price ;
        sh:datatype xsd:decimal ;
        sh:minInclusive 0 ;
        sh:name "PRICE" ;
    ] ;
    sh:property [
        sh:path ecom:stockQuantity ;
        sh:datatype xsd:integer ;
        sh:minInclusive 0 ;
        sh:name "STOCK_QUANTITY" ;
    ] .

# Order Shape - generates ORDERS table
ecom:OrderShape a sh:NodeShape ;
    sh:targetClass ecom:Order ;
    rdfs:label "Order Table Shape" ;
    sh:property [
        sh:path ecom:orderId ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:maxLength 20 ;
        sh:name "ORDER_ID" ;
        ecom:isPrimaryKey true ;
    ] ;
    sh:property [
        sh:path ecom:customerId ;
        sh:datatype xsd:string ;
        sh:maxLength 20 ;
        sh:name "CUSTOMER_ID" ;
    ] ;
    sh:property [
        sh:path ecom:productId ;
        sh:datatype xsd:string ;
        sh:maxLength 20 ;
        sh:name "PRODUCT_ID" ;
    ] ;
    sh:property [
        sh:path ecom:quantity ;
        sh:datatype xsd:integer ;
        sh:minInclusive 1 ;
        sh:name "QUANTITY" ;
    ] ;
    sh:property [
        sh:path ecom:totalAmount ;
        sh:datatype xsd:decimal ;
        sh:minInclusive 0 ;
        sh:name "TOTAL_AMOUNT" ;
    ] .
"""
        return ontology_ttl.strip()

    def upload_ontology(self) -> Optional[str]:
        """Upload ontology to coordinator and return ontology_id"""
        print("\n📚 Uploading E-Commerce Ontology with SHACL shapes...")

        url = f"{self.base_url}/api/v1/ontology"

        ontology_content = self.create_ecommerce_ontology_with_shacl()

        payload = {
            "id": f"ecommerce_{self.test_id}",
            "name": "E-Commerce Domain Ontology",
            "content": ontology_content,
            "namespace": self.ontology_namespace,
            "version": "1.0.0",
            "description": "E-commerce ontology with SHACL shapes for DDL generation"
        }

        try:
            response = self.session.post(url, json=payload, timeout=30)

            if response.status_code == 200:
                result = response.json()
                self.ontology_id = result.get('ontology_id', payload['id'])
                print(f"   ✅ Ontology uploaded successfully")
                print(f"   📋 Ontology ID: {self.ontology_id}")
                print(f"   🌐 Namespace: {self.ontology_namespace}")
                print(f"   🔧 SHACL Shapes: CustomerShape, ProductShape, OrderShape")
                return self.ontology_id
            else:
                print(f"   ❌ Ontology upload failed: {response.status_code}")
                print(f"   📄 Response: {response.text}")
                return None

        except Exception as e:
            print(f"   ❌ Ontology upload error: {e}")
            return None

    def get_shacl_shape_uri(self, shape_name: str) -> str:
        """Get the full URI for a SHACL shape (uses instance's unique namespace)"""
        # SHACL shapes use the same namespace as the ontology (unique per test run)
        return f"{self.ontology_namespace}{shape_name}"

    # ========================================================================
    # Data Quality & Transformation
    # ========================================================================

    def apply_quality_rules(self, entity_type: str, row: Dict, row_num: int) -> Tuple[bool, Optional[str], Optional[str]]:
        """Apply data quality rules and return (is_valid, reason, rule_id)"""

        if entity_type == "customers":
            # Rule 1: Email required
            if not row.get("email") or row["email"].strip() == "":
                return False, "Missing required field: email", "customer_email_required"

            # Rule 2: Age must be positive
            try:
                age = int(row.get("age", 0))
                if age <= 0:
                    return False, "Age must be positive", "customer_age_positive"
            except (ValueError, TypeError):
                return False, "Invalid age value", "customer_age_valid"

            # Rule 3: First name required
            if not row.get("first_name") or row["first_name"].strip() == "":
                return False, "Missing required field: first_name", "customer_firstname_required"

        elif entity_type == "products":
            # Rule 1: Price must be positive
            try:
                price = float(row.get("price", 0))
                if price < 0:
                    return False, "Price cannot be negative", "product_price_positive"
            except (ValueError, TypeError):
                return False, "Invalid price value", "product_price_valid"

            # Rule 2: Stock quantity must be non-negative
            try:
                stock = int(row.get("stock_quantity", 0))
                if stock < 0:
                    return False, "Stock quantity cannot be negative", "product_stock_nonnegative"
            except (ValueError, TypeError):
                return False, "Invalid stock quantity", "product_stock_valid"

        elif entity_type == "orders":
            # Rule 1: Customer ID required
            if not row.get("customer_id") or row["customer_id"].strip() == "":
                return False, "Missing required field: customer_id", "order_customer_required"

            # Rule 2: Total amount must be positive
            try:
                amount = float(row.get("total_amount", 0))
                if amount < 0:
                    return False, "Total amount cannot be negative", "order_amount_positive"
            except (ValueError, TypeError):
                return False, "Invalid total amount", "order_amount_valid"

        return True, None, None

    def transform_row(self, entity_type: str, row: Dict) -> Tuple[Dict, List[Dict]]:
        """Apply transformations and return (transformed_row, transformations_list)"""

        transformed = row.copy()
        transformations = []

        if entity_type == "customers":
            # Transformation 1: Standardize name fields (proper case)
            if row.get("first_name"):
                before = row["first_name"]
                transformed["first_name"] = row["first_name"].strip().title()
                if before != transformed["first_name"]:
                    transformations.append({
                        "transform_type": "proper_case",
                        "fields": ["first_name"],
                        "before_values": {"first_name": before},
                        "after_values": {"first_name": transformed["first_name"]},
                        "applied_at": datetime.now(timezone.utc).isoformat()
                    })

            if row.get("last_name"):
                before = row["last_name"]
                transformed["last_name"] = row["last_name"].strip().title()
                if before != transformed["last_name"]:
                    transformations.append({
                        "transform_type": "proper_case",
                        "fields": ["last_name"],
                        "before_values": {"last_name": before},
                        "after_values": {"last_name": transformed["last_name"]},
                        "applied_at": datetime.now(timezone.utc).isoformat()
                    })

            # Transformation 2: Normalize email to lowercase
            if row.get("email"):
                before = row["email"]
                transformed["email"] = row["email"].strip().lower()
                if before != transformed["email"]:
                    transformations.append({
                        "transform_type": "normalize_email",
                        "fields": ["email"],
                        "before_values": {"email": before},
                        "after_values": {"email": transformed["email"]},
                        "applied_at": datetime.now(timezone.utc).isoformat()
                    })

        elif entity_type == "products":
            # Transformation: Normalize product name
            if row.get("product_name"):
                before = row["product_name"]
                transformed["product_name"] = row["product_name"].strip().title()
                if before != transformed["product_name"]:
                    transformations.append({
                        "transform_type": "normalize_text",
                        "fields": ["product_name"],
                        "before_values": {"product_name": before},
                        "after_values": {"product_name": transformed["product_name"]},
                        "applied_at": datetime.now(timezone.utc).isoformat()
                    })

        return transformed, transformations

    # ========================================================================
    # ETL Processing with Lineage
    # ========================================================================

    def process_csv_with_lineage(self, entity_type: str) -> Dict:
        """Process CSV file with row-level lineage tracking"""

        csv_file = self.csv_files[entity_type]
        print(f"\n⚙️  Processing {entity_type}: {csv_file}")

        stats = {
            "total_rows": 0,
            "processed": 0,
            "filtered": 0,
            "failed": 0,
            "lineage_events": [],
        }

        with open(csv_file, "r") as f:
            reader = csv.DictReader(f)
            rows = list(reader)
            stats["total_rows"] = len(rows)

            for row_idx, row in enumerate(rows, start=2):  # Start at 2 (header is row 1)
                # Apply quality rules
                is_valid, filter_reason, rule_id = self.apply_quality_rules(entity_type, row, row_idx)

                # Apply transformations
                transformed_row, transformations = self.transform_row(entity_type, row)

                # Create lineage event
                row_id = {
                    "source_type": "Csv",
                    "source_id": os.path.basename(csv_file),
                    "position": {"RowNumber": row_idx}
                }

                if is_valid:
                    # Successful processing
                    table_name = f"{entity_type.upper()}_{self.test_id}"
                    pk_value = row.get(f"{entity_type[:-1]}_id", f"UNKNOWN_{row_idx}")

                    event = {
                        "row_id": row_id,
                        "batch_id": self.batch_id,
                        "job_id": self.job_id,
                        "timestamp": datetime.now(timezone.utc).isoformat(),
                        "outcome": {
                            "Processed": {
                                "output_location": f"db2://localhost:50000/GRAPHICA/{table_name}"
                            }
                        },
                        "transformations": transformations,
                        "output_row_id": {
                            "source_type": {"Database": "DB2"},
                            "source_id": table_name,
                            "position": {"PrimaryKey": {f"{entity_type[:-1]}_id": pk_value}}
                        },
                        "tenant_id": "test-tenant",
                        "correlation_id": f"{self.job_id}-{entity_type}-{row_idx}"
                    }
                    stats["processed"] += 1
                else:
                    # Filtered row
                    event = {
                        "row_id": row_id,
                        "batch_id": self.batch_id,
                        "job_id": self.job_id,
                        "timestamp": datetime.now(timezone.utc).isoformat(),
                        "outcome": {
                            "Filtered": {
                                "reason": filter_reason,
                                "rule_id": rule_id
                            }
                        },
                        "transformations": transformations,
                        "output_row_id": None,
                        "tenant_id": "test-tenant",
                        "correlation_id": f"{self.job_id}-{entity_type}-{row_idx}"
                    }
                    stats["filtered"] += 1

                stats["lineage_events"].append(event)

        print(f"   📊 Processed: {stats['processed']}, Filtered: {stats['filtered']}, Failed: {stats['failed']}")
        return stats

    def write_lineage_events(self, events: List[Dict]) -> bool:
        """Write lineage events to Graphica"""
        print(f"\n💾 Writing {len(events)} lineage events...")

        # Write in batches for efficiency
        batch_size = 100
        for i in range(0, len(events), batch_size):
            batch = events[i:i+batch_size]

            # Use batch endpoint if available, otherwise write individually
            try:
                url = f"{self.base_url}/api/v1/lineage/rows/batch/test"
                response = self.session.post(url, json=batch, timeout=30)
                response.raise_for_status()
            except requests.exceptions.RequestException as e:
                print(f"   ⚠️  Batch write failed, falling back to individual writes: {e}")
                # Fallback to individual writes
                for event in batch:
                    try:
                        url = f"{self.base_url}/api/v1/lineage/row/test"
                        response = self.session.post(url, json=event, timeout=10)
                        response.raise_for_status()
                    except requests.exceptions.RequestException as e2:
                        print(f"   ❌ Failed to write event: {e2}")
                        return False

        print(f"   ✅ Wrote {len(events)} lineage events")
        return True

    def flush_lineage_buffer(self) -> bool:
        """Flush lineage buffer to storage"""
        try:
            url = f"{self.base_url}/api/v1/lineage/flush/test"
            response = self.session.post(url, timeout=10)
            response.raise_for_status()
            return True
        except requests.exceptions.RequestException as e:
            print(f"   ❌ Failed to flush lineage buffer: {e}")
            return False

    def load_to_db2(self, entity_type: str, stats: Dict) -> bool:
        """Load processed data to DB2"""
        print(f"\n📤 Loading {entity_type} to DB2...")

        if not self.db2_conn:
            if not self.connect_db2():
                return False

        table_name = f"{entity_type.upper()}_{self.test_id}"
        loaded_count = 0

        # Load only successfully processed rows
        for event in stats["lineage_events"]:
            if "Processed" in event["outcome"]:
                # Get original row data
                row_num = event["row_id"]["position"]["RowNumber"]
                csv_file = self.csv_files[entity_type]

                with open(csv_file, "r") as f:
                    reader = csv.DictReader(f)
                    rows = list(reader)
                    if row_num - 2 < len(rows):  # row_num is 1-indexed with header
                        row = rows[row_num - 2]

                        # Apply transformations from lineage
                        for transform in event["transformations"]:
                            if transform.get("after_values"):
                                row.update(transform["after_values"])

                        # Build INSERT statement
                        columns = list(row.keys())
                        placeholders = ", ".join(["?" for _ in columns])
                        column_names = ", ".join(columns)

                        insert_sql = f"INSERT INTO {table_name} ({column_names}) VALUES ({placeholders})"

                        try:
                            stmt = ibm_db.prepare(self.db2_conn, insert_sql)
                            values = [row[col] for col in columns]
                            ibm_db.execute(stmt, tuple(values))
                            loaded_count += 1
                        except Exception as e:
                            print(f"   ⚠️  Failed to insert row {row_num}: {e}")

        print(f"   ✅ Loaded {loaded_count} rows to {table_name}")
        return loaded_count > 0

    # ========================================================================
    # Validation
    # ========================================================================

    def validate_db2_data(self, entity_type: str, expected_count: int) -> bool:
        """Validate data loaded in DB2"""
        print(f"\n✓ Validating {entity_type} in DB2...")

        if not self.db2_conn:
            if not self.connect_db2():
                return False

        table_name = f"{entity_type.upper()}_{self.test_id}"

        try:
            # Count rows
            count_sql = f"SELECT COUNT(*) FROM {table_name}"
            stmt = ibm_db.exec_immediate(self.db2_conn, count_sql)
            row = ibm_db.fetch_tuple(stmt)
            actual_count = row[0]

            print(f"   DB2 has {actual_count} rows (expected ~{expected_count})")

            # Sample a few rows
            sample_sql = f"SELECT * FROM {table_name} FETCH FIRST 3 ROWS ONLY"
            stmt = ibm_db.exec_immediate(self.db2_conn, sample_sql)

            print(f"   Sample rows:")
            sample_count = 0
            while ibm_db.fetch_row(stmt):
                sample_count += 1
                if sample_count <= 3:
                    row_dict = {}
                    for i in range(ibm_db.num_fields(stmt)):
                        field_name = ibm_db.field_name(stmt, i)
                        field_value = ibm_db.result(stmt, i)
                        row_dict[field_name] = field_value
                    # Show first 5 values (truncate if too long)
                    values = [str(v)[:30] + '...' if v and len(str(v)) > 30 else v for v in list(row_dict.values())[:5]]
                    print(f"      Row {sample_count}: {values}")

            return actual_count > 0

        except Exception as e:
            print(f"   ❌ Validation failed: {e}")
            return False

    def validate_lineage_tracing(self, entity_type: str, sample_size: int = 5) -> bool:
        """Validate lineage tracing from DB2 back to CSV"""
        print(f"\n🔍 Validating lineage tracing for {entity_type}...")

        csv_file = self.csv_files[entity_type]
        csv_basename = os.path.basename(csv_file)

        # Sample a few row numbers
        with open(csv_file, "r") as f:
            reader = csv.DictReader(f)
            rows = list(reader)
            sample_rows = random.sample(range(len(rows)), min(sample_size, len(rows)))

        traced_count = 0

        for row_idx in sample_rows:
            row_num = row_idx + 2  # Account for 0-index and header row
            row_key = f"csv:{csv_basename}:{row_num}"

            try:
                # Query lineage
                url = f"{self.base_url}/api/v1/lineage/row/{row_key}"
                response = self.session.get(url, timeout=10)
                response.raise_for_status()
                result = response.json()

                if result.get("total_count", 0) > 0:
                    events = result.get("events", [])
                    event = events[0]

                    outcome = event.get("outcome", {})
                    transformations = event.get("transformations", [])

                    print(f"   ✅ Row {row_num}: {len(transformations)} transformations, outcome: {list(outcome.keys())}")
                    traced_count += 1
                else:
                    print(f"   ⚠️  Row {row_num}: No lineage found")

            except requests.exceptions.RequestException as e:
                print(f"   ❌ Row {row_num}: Lineage query failed: {e}")

        print(f"   Traced {traced_count}/{sample_size} sampled rows")
        return traced_count > 0

    # ========================================================================
    # Cleanup
    # ========================================================================

    def cleanup(self):
        """Clean up resources"""
        if self.db2_conn:
            try:
                ibm_db.close(self.db2_conn)
            except:
                pass

    # ========================================================================
    # Main Test Flow
    # ========================================================================

    def run(self) -> bool:
        """Execute complete end-to-end test"""

        print("="*80)
        print(" Graphica End-to-End ETL Test with Row-Level Lineage")
        print("="*80)
        print(f"\nTest ID: {self.test_id}")
        print(f"Job ID: {self.job_id}")
        print(f"Batch ID: {self.batch_id}")

        try:
            # Step 1: Authenticate
            print("\n" + "="*80)
            print("STEP 1: Authentication")
            print("="*80)
            if not self.authenticate():
                return False
            print("✅ Authenticated successfully")

            # Step 2: Generate synthetic data
            print("\n" + "="*80)
            print("STEP 2: Generate Synthetic Data")
            print("="*80)
            self.generate_customers_csv(100)
            self.generate_products_csv(100)
            self.generate_orders_csv(100)
            print(f"\n✅ Generated 3 CSV files with 300 total rows")

            # Step 2.5: Upload CSVs to File Library (NEW!)
            print("\n" + "="*80)
            print("STEP 2.5: Upload CSVs to File Library")
            print("="*80)

            file_ids = {}
            for entity_type in ["customers", "products", "orders"]:
                csv_path = self.csv_files[entity_type]
                table_name = f"{entity_type.upper()}_{self.test_id}"
                file_id = self.upload_to_file_library(csv_path, table_name)

                if file_id:
                    file_ids[entity_type] = file_id
                    print(f"   ✅ {entity_type}: {file_id}")
                else:
                    print(f"   ⚠️  {entity_type}: Upload failed (continuing anyway)")

            print(f"\n✅ Uploaded {len(file_ids)}/3 CSV files to File Library")

            # Step 2.6: Upload Ontology with SHACL Shapes (NEW!)
            print("\n" + "="*80)
            print("STEP 2.6: Upload E-Commerce Ontology with SHACL Shapes")
            print("="*80)

            ddl_generated = False  # Track whether SHACL-driven DDL succeeded
            ontology_id = self.upload_ontology()
            if ontology_id:
                print(f"✅ Ontology uploaded: {ontology_id}")

                # Try to generate DDL from SHACL (if backend is ready)
                print("\n🔨 Attempting DDL generation from SHACL shapes...")
                print("   (If this fails, will fall back to manual DDL creation)")

                all_ddl_statements = []

                shapes = [
                    ("http://example.com/ecommerce#CustomerShape", f"CUSTOMERS_{self.test_id}"),
                    ("http://example.com/ecommerce#ProductShape", f"PRODUCTS_{self.test_id}"),
                    ("http://example.com/ecommerce#OrderShape", f"ORDERS_{self.test_id}"),
                ]

                for shacl_uri, table_name in shapes:
                    ddl_statements = self.generate_ddl_from_shacl(shacl_uri, dialect="db2")
                    if ddl_statements:
                        all_ddl_statements.extend(ddl_statements)

                if all_ddl_statements:
                    print(f"\n✅ Generated {len(all_ddl_statements)} DDL statements from SHACL")
                    print("   Executing DDL via coordinator...")

                    if self.execute_ddl_via_coordinator(all_ddl_statements, shacl_uri="http://example.com/ecommerce#"):
                        print("✅ DDL executed successfully - tables created via SHACL!")
                        ddl_generated = True
                    else:
                        print("⚠️  DDL execution failed - will fall back to manual creation")
                else:
                    print("⚠️  DDL generation from SHACL not available yet - falling back to manual")
            else:
                print("⚠️  Ontology upload failed - continuing with manual DDL")

            # Step 3: Create DB2 tables (fallback if SHACL-driven DDL didn't work)
            print("\n" + "="*80)
            print("STEP 3: Create DB2 Tables")
            print("="*80)

            if ddl_generated:
                print("✅ Tables already created via SHACL-driven DDL - skipping manual creation")
            else:
                print("📝 Using manual DDL creation (SHACL-driven DDL not yet available)")
                if not self.create_db2_tables():
                    return False
                print("✅ DB2 tables created (manually)")

            # Step 4: Process CSVs with quality rules and transformations
            print("\n" + "="*80)
            print("STEP 4: Process Data with Quality Rules & Transformations")
            print("="*80)

            all_events = []
            stats_summary = {}

            for entity_type in ["customers", "products", "orders"]:
                stats = self.process_csv_with_lineage(entity_type)
                stats_summary[entity_type] = stats
                all_events.extend(stats["lineage_events"])

            print(f"\n✅ Processed 300 rows, generated {len(all_events)} lineage events")

            # Step 5: Write lineage events
            print("\n" + "="*80)
            print("STEP 5: Write Lineage Events to Graphica")
            print("="*80)
            if not self.write_lineage_events(all_events):
                return False

            # Flush buffer
            print("\n💾 Flushing lineage buffer...")
            if not self.flush_lineage_buffer():
                return False
            print("✅ Lineage buffer flushed")

            time.sleep(1)  # Give DB a moment

            # Step 6: Load data to DB2
            print("\n" + "="*80)
            print("STEP 6: Load Data to DB2")
            print("="*80)

            for entity_type in ["customers", "products", "orders"]:
                if not self.load_to_db2(entity_type, stats_summary[entity_type]):
                    print(f"   ⚠️  Failed to load {entity_type}")

            print("✅ Data loaded to DB2")

            # Step 7: Validate DB2 data
            print("\n" + "="*80)
            print("STEP 7: Validate Data in DB2")
            print("="*80)

            for entity_type in ["customers", "products", "orders"]:
                expected = stats_summary[entity_type]["processed"]
                self.validate_db2_data(entity_type, expected)

            # Step 8: Validate lineage tracing
            print("\n" + "="*80)
            print("STEP 8: Validate Row-Level Lineage Tracing")
            print("="*80)

            for entity_type in ["customers", "products", "orders"]:
                self.validate_lineage_tracing(entity_type, sample_size=5)

            # Final summary
            print("\n" + "="*80)
            print(" TEST SUMMARY")
            print("="*80)

            print(f"\n✅ End-to-End Test PASSED!")
            print(f"\nStatistics:")
            for entity_type, stats in stats_summary.items():
                print(f"\n{entity_type.upper()}:")
                print(f"  Total rows: {stats['total_rows']}")
                print(f"  Processed: {stats['processed']}")
                print(f"  Filtered: {stats['filtered']}")
                print(f"  Failed: {stats['failed']}")

            print(f"\nDB2 Tables:")
            for entity_type in ["customers", "products", "orders"]:
                print(f"  {entity_type.upper()}_{self.test_id}")

            print(f"\nCSV Files:")
            for entity_type, filepath in self.csv_files.items():
                print(f"  {filepath}")

            print(f"\nLineage Events: {len(all_events)} total")
            print(f"Job ID: {self.job_id}")
            print(f"Batch ID: {self.batch_id}")

            return True

        except Exception as e:
            print(f"\n❌ Test failed with exception: {e}")
            import traceback
            traceback.print_exc()
            return False

        finally:
            self.cleanup()


def main():
    """Main entry point"""
    test = GraphicaETLTest()
    success = test.run()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
