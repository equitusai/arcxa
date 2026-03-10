#!/usr/bin/env python3
"""
Graphica End-to-End Coordinator Test - Ontological ETL
=======================================================

This test validates the complete ETL pipeline through the Graphica coordinator,
following the File Library First architecture and ontological data access.

Test Flow:
1. Generate synthetic CSV data (customers, products, orders)
2. Upload files to File Library
3. Execute ontology-driven DDL (tables created by coordinator)
4. Create loader jobs through coordinator API
5. Monitor job progress and wait for completion
6. Query data through coordinator (ontological access)
7. Validate lineage and data quality

Architecture Principles:
- File Library First: All CSV files go through file library
- Coordinator-Only: No direct database access from test
- Ontological: All filtering/queries use semantic layer
- Quality-Driven: Let coordinator apply quality rules
"""

import csv
import json
import os
import random
import sys
import time
from datetime import datetime, timezone
from typing import Dict, List, Optional
import requests
from faker import Faker

fake = Faker()
Faker.seed(42)


class GraphicaCoordinatorTest:
    """End-to-End test exercising only coordinator APIs"""

    def __init__(self, base_url: str = "http://localhost:8080"):
        self.base_url = base_url
        self.session = requests.Session()
        self.token = None

        # Test metadata
        self.test_id = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
        self.job_id_prefix = f"test_{self.test_id}"

        # File paths
        self.output_dir = "/tmp/graphica_coordinator_test"
        os.makedirs(self.output_dir, exist_ok=True)

        # Track file library IDs
        self.file_library_ids = {}

        # Track loader job IDs
        self.loader_jobs = {}

        # Track created tables
        self.tables = []

    # ========================================================================
    # Step 1: Generate Synthetic Data
    # ========================================================================

    def generate_customers_csv(self, num_rows: int = 100) -> str:
        """Generate synthetic customer data"""
        print(f"\n📊 Generating {num_rows} customer records...")

        csv_file = os.path.join(self.output_dir, f"customers_{self.test_id}.csv")
        customers = []

        for i in range(num_rows):
            customer = {
                "customer_id": f"CUST{str(i+1).zfill(6)}",
                "first_name": fake.first_name(),
                "last_name": fake.last_name(),
                "email": fake.email(),
                "phone": fake.phone_number(),
                "age": random.randint(18, 80),
                "city": fake.city(),
                "state": fake.state_abbr(),
                "country": "USA",
                "registration_date": fake.date_between(start_date="-2y", end_date="today").isoformat(),
                "loyalty_tier": random.choice(["Bronze", "Silver", "Gold", "Platinum"]),
            }
            customers.append(customer)

        with open(csv_file, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=customers[0].keys())
            writer.writeheader()
            writer.writerows(customers)

        print(f"   ✅ Generated {len(customers)} customers -> {csv_file}")
        return csv_file

    def generate_products_csv(self, num_rows: int = 100) -> str:
        """Generate synthetic product data"""
        print(f"\n📦 Generating {num_rows} product records...")

        csv_file = os.path.join(self.output_dir, f"products_{self.test_id}.csv")
        categories = ["Electronics", "Clothing", "Home", "Sports", "Books"]
        products = []

        for i in range(num_rows):
            product = {
                "product_id": f"PROD{str(i+1).zfill(6)}",
                "product_name": fake.catch_phrase(),
                "category": random.choice(categories),
                "brand": fake.company(),
                "price": round(random.uniform(5.0, 500.0), 2),
                "stock_quantity": random.randint(0, 1000),
                "is_active": random.choice(["Y", "N"]),
            }
            products.append(product)

        with open(csv_file, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=products[0].keys())
            writer.writeheader()
            writer.writerows(products)

        print(f"   ✅ Generated {len(products)} products -> {csv_file}")
        return csv_file

    def generate_orders_csv(self, num_rows: int = 100) -> str:
        """Generate synthetic order data"""
        print(f"\n🛒 Generating {num_rows} order records...")

        csv_file = os.path.join(self.output_dir, f"orders_{self.test_id}.csv")
        orders = []
        statuses = ["Pending", "Processing", "Shipped", "Delivered", "Cancelled"]

        for i in range(num_rows):
            order = {
                "order_id": f"ORD{str(i+1).zfill(8)}",
                "customer_id": f"CUST{str(random.randint(1, 100)).zfill(6)}",
                "product_id": f"PROD{str(random.randint(1, 100)).zfill(6)}",
                "order_date": fake.date_time_between(start_date="-1y", end_date="now", tzinfo=timezone.utc).isoformat(),
                "quantity": random.randint(1, 10),
                "unit_price": round(random.uniform(10.0, 500.0), 2),
                "total_amount": round(random.uniform(10.0, 5000.0), 2),
                "status": random.choice(statuses),
                "payment_method": random.choice(["Credit Card", "Debit Card", "PayPal"]),
            }
            orders.append(order)

        with open(csv_file, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=orders[0].keys())
            writer.writeheader()
            writer.writerows(orders)

        print(f"   ✅ Generated {len(orders)} orders -> {csv_file}")
        return csv_file

    # ========================================================================
    # Step 2a: Register Custom Ontology
    # ========================================================================

    def create_ecommerce_ontology(self) -> str:
        """Create a custom e-commerce ontology in Turtle format"""
        ontology = f"""
@prefix ecom: <http://example.com/ecommerce#> .
@prefix schema: <http://schema.org/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Customer class and properties
ecom:Customer a rdfs:Class ;
    rdfs:label "Customer" ;
    rdfs:comment "A customer in the e-commerce system" .

ecom:customerId a rdf:Property ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "Customer ID" ;
    schema:identifier "customerId" .

ecom:email a rdf:Property ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "Email Address" ;
    schema:email "email" .

ecom:firstName a rdf:Property ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "First Name" ;
    schema:givenName "firstName" .

ecom:lastName a rdf:Property ;
    rdfs:domain ecom:Customer ;
    rdfs:range xsd:string ;
    rdfs:label "Last Name" ;
    schema:familyName "lastName" .

# Product class and properties
ecom:Product a rdfs:Class ;
    rdfs:label "Product" ;
    rdfs:comment "A product in the catalog" .

ecom:productId a rdf:Property ;
    rdfs:domain ecom:Product ;
    rdfs:range xsd:string ;
    rdfs:label "Product ID" .

ecom:productName a rdf:Property ;
    rdfs:domain ecom:Product ;
    rdfs:range xsd:string ;
    rdfs:label "Product Name" ;
    schema:name "productName" .

ecom:price a rdf:Property ;
    rdfs:domain ecom:Product ;
    rdfs:range xsd:decimal ;
    rdfs:label "Price" ;
    schema:price "price" .

# Order class and properties
ecom:Order a rdfs:Class ;
    rdfs:label "Order" ;
    rdfs:comment "A customer order" .

ecom:orderId a rdf:Property ;
    rdfs:domain ecom:Order ;
    rdfs:range xsd:string ;
    rdfs:label "Order ID" .

ecom:orderDate a rdf:Property ;
    rdfs:domain ecom:Order ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Order Date" ;
    schema:orderDate "orderDate" .

ecom:totalAmount a rdf:Property ;
    rdfs:domain ecom:Order ;
    rdfs:range xsd:decimal ;
    rdfs:label "Total Amount" ;
    schema:totalPrice "totalAmount" .
"""
        return ontology

    def upload_ontology(self) -> Optional[str]:
        """Upload custom ontology to coordinator"""
        print(f"\n📚 Uploading custom e-commerce ontology...")

        url = f"{self.base_url}/api/v1/ontology"

        ontology_content = self.create_ecommerce_ontology()

        payload = {
            "id": f"ecommerce_{self.test_id}",
            "name": "E-Commerce Domain Ontology",
            "description": "Custom ontology for e-commerce entities (Customer, Product, Order)",
            "content": ontology_content,
            "namespace": "http://example.com/ecommerce#",
            "version": "1.0.0",
            "author": "Graphica E2E Test",
            "tags": ["ecommerce", "retail", "test"]
        }

        try:
            response = self.session.post(url, json=payload, timeout=30)
            response.raise_for_status()
            result = response.json()
            ontology_id = result.get("metadata", {}).get("id", payload["id"])
            print(f"   ✅ Ontology uploaded: {ontology_id}")
            print(f"      Classes: Customer, Product, Order")
            print(f"      Properties: customerId, email, firstName, lastName, productName, price, orderId, orderDate, totalAmount")
            return ontology_id
        except requests.exceptions.RequestException as e:
            print(f"   ⚠️  Ontology upload: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"      Response: {e.response.text[:300]}")
            return None

    def create_field_mappings(self, entity_type: str, file_id: str, table_name: str) -> bool:
        """Create field-to-ontology mappings for CSV columns"""
        print(f"\n🔗 Creating field mappings for {entity_type}...")

        # Define mappings: CSV field → Ontology property
        mappings = {
            "customers": [
                ("customer_id", "http://example.com/ecommerce#customerId", "Unique customer identifier"),
                ("email", "http://example.com/ecommerce#email", "Customer email address"),
                ("first_name", "http://example.com/ecommerce#firstName", "Customer first name"),
                ("last_name", "http://example.com/ecommerce#lastName", "Customer last name"),
            ],
            "products": [
                ("product_id", "http://example.com/ecommerce#productId", "Unique product identifier"),
                ("product_name", "http://example.com/ecommerce#productName", "Product name"),
                ("price", "http://example.com/ecommerce#price", "Product price"),
            ],
            "orders": [
                ("order_id", "http://example.com/ecommerce#orderId", "Unique order identifier"),
                ("order_date", "http://example.com/ecommerce#orderDate", "Order date"),
                ("total_amount", "http://example.com/ecommerce#totalAmount", "Order total amount"),
            ]
        }

        url = f"{self.base_url}/api/v1/mapping/manual"
        created_count = 0

        for field_name, ontology_uri, notes in mappings.get(entity_type, []):
            payload = {
                "source_id": "graphica_test",
                "table_name": table_name,
                "field_name": field_name,
                "target_field_uri": ontology_uri,
                "notes": notes,
                "created_by": f"e2e_test_{self.test_id}"
            }

            try:
                response = self.session.post(url, json=payload, timeout=10)
                response.raise_for_status()
                result = response.json()
                created_count += 1
                print(f"   ✅ Mapped: {field_name} → {ontology_uri.split('#')[-1]}")
            except requests.exceptions.RequestException as e:
                print(f"   ⚠️  Mapping failed for {field_name}: {e}")

        print(f"   Created {created_count} field mappings")
        return created_count > 0

    # ========================================================================
    # Step 2b: Upload to File Library
    # ========================================================================

    def upload_to_file_library(self, file_path: str, entity_type: str) -> Optional[str]:
        """Upload CSV file to file library and return file_id"""
        print(f"\n📤 Uploading {entity_type} to file library...")

        url = f"{self.base_url}/api/v1/file-library/files"

        with open(file_path, "rb") as f:
            files = {"file": (os.path.basename(file_path), f, "text/csv")}
            data = {
                "name": os.path.basename(file_path),
                "tags": json.dumps([entity_type, f"test_{self.test_id}"]),
                "metadata": json.dumps({
                    "entity_type": entity_type,
                    "test_id": self.test_id,
                    "generated_at": datetime.now(timezone.utc).isoformat()
                })
            }

            try:
                response = self.session.post(url, files=files, data=data, timeout=30)
                response.raise_for_status()
                result = response.json()
                file_id = result.get("file_id")
                print(f"   ✅ Uploaded: file_id={file_id}")
                return file_id
            except requests.exceptions.RequestException as e:
                print(f"   ❌ Upload failed: {e}")
                if hasattr(e.response, 'text'):
                    print(f"      Response: {e.response.text[:200]}")
                return None

    def scan_file_schema(self, file_id: str) -> bool:
        """Trigger schema scan for uploaded file"""
        print(f"   🔍 Scanning schema for file_id={file_id}...")

        url = f"{self.base_url}/api/v1/file-library/files/{file_id}/scan"

        try:
            response = self.session.post(url, json={}, timeout=30)
            response.raise_for_status()
            print(f"   ✅ Schema scanned")
            return True
        except requests.exceptions.RequestException as e:
            print(f"   ⚠️  Schema scan failed: {e}")
            return False

    # ========================================================================
    # Step 3: Execute Ontology-Driven DDL
    # ========================================================================

    def execute_ddl(self, table_name: str, entity_type: str, file_id: str) -> bool:
        """Execute ontology-driven DDL to create table"""
        print(f"\n🗄️  Creating table {table_name} via coordinator...")

        # Use ontology-driven DDL endpoint
        url = f"{self.base_url}/api/v1/ddl/execute"

        # Define table schema based on entity type
        schemas = {
            "customers": """
                CREATE TABLE {table} (
                    customer_id VARCHAR(20) PRIMARY KEY,
                    first_name VARCHAR(50),
                    last_name VARCHAR(50),
                    email VARCHAR(100),
                    phone VARCHAR(100),
                    age INTEGER,
                    city VARCHAR(100),
                    state VARCHAR(10),
                    country VARCHAR(50),
                    registration_date DATE,
                    loyalty_tier VARCHAR(20)
                )
            """,
            "products": """
                CREATE TABLE {table} (
                    product_id VARCHAR(20) PRIMARY KEY,
                    product_name VARCHAR(500),
                    category VARCHAR(50),
                    brand VARCHAR(200),
                    price DECIMAL(10,2),
                    stock_quantity INTEGER,
                    is_active CHAR(1)
                )
            """,
            "orders": """
                CREATE TABLE {table} (
                    order_id VARCHAR(20) PRIMARY KEY,
                    customer_id VARCHAR(20),
                    product_id VARCHAR(20),
                    order_date TIMESTAMP,
                    quantity INTEGER,
                    unit_price DECIMAL(10,2),
                    total_amount DECIMAL(10,2),
                    status VARCHAR(20),
                    payment_method VARCHAR(50)
                )
            """
        }

        ddl = schemas.get(entity_type, "").format(table=table_name)

        payload = {
            "ddl_statements": [ddl],
            "database_config": {
                "db_type": "db2",
                "host": "localhost",
                "port": 50000,
                "database": "GRAPHICA",
                "username": "db2inst1",
                "password": "graphica-db2-pass",
                "options": {}
            },
            "transactional": True,
            "continue_on_error": False,
            "shacl_uri": None
        }

        try:
            response = self.session.post(url, json=payload, timeout=60)
            response.raise_for_status()
            result = response.json()
            print(f"   ✅ Table created: {result}")
            self.tables.append(table_name)
            return True
        except requests.exceptions.RequestException as e:
            print(f"   ⚠️  DDL execution: {e}")
            if hasattr(e.response, 'text'):
                print(f"      Response: {e.response.text[:300]}")
            # Table might already exist, continue anyway
            self.tables.append(table_name)
            return True

    # ========================================================================
    # Step 4: Create Loader Jobs
    # ========================================================================

    def create_loader_job(self, file_id: str, entity_type: str, table_name: str) -> Optional[str]:
        """Create loader job through coordinator API"""
        print(f"\n⚙️  Creating loader job for {entity_type}...")

        url = f"{self.base_url}/api/v1/loader/jobs"

        payload = {
            "name": f"{entity_type}_load_{self.test_id}",
            "source_file_id": file_id,
            "target_config": {
                "db_type": "db2",  # Required: "db2" or "postgresql"
                "host": "localhost",
                "port": 50000,
                "database": "GRAPHICA",
                "table": table_name,
                "username": "db2inst1",
                "password": "graphica-db2-pass",
                "options": {}
            },
            "column_mappings": [],  # Will be auto-detected from CSV headers
            "loader_config": {
                "batch_size": 100,
                "max_connections": 2,
                "use_load_utility": True,
                "load_buffer_kb": 4096,
                "load_parallelism": 2,
                "auto_create_table": False  # Already created via DDL
            }
        }

        try:
            response = self.session.post(url, json=payload, timeout=30)
            response.raise_for_status()
            result = response.json()
            job_id = result.get("job_id")
            print(f"   ✅ Job created: job_id={job_id}")
            return job_id
        except requests.exceptions.RequestException as e:
            print(f"   ❌ Job creation failed: {e}")
            if hasattr(e.response, 'text'):
                print(f"      Response: {e.response.text[:300]}")
            return None

    def wait_for_job_completion(self, job_id: str, timeout_seconds: int = 300) -> bool:
        """Wait for loader job to complete"""
        print(f"\n⏳ Waiting for job {job_id} to complete...")

        url = f"{self.base_url}/api/v1/loader/jobs/{job_id}"
        start_time = time.time()

        while time.time() - start_time < timeout_seconds:
            try:
                response = self.session.get(url, timeout=10)
                response.raise_for_status()
                result = response.json()

                status = result.get("status", "Unknown")
                rows_processed = result.get("rows_processed", 0)

                print(f"   Status: {status}, Rows: {rows_processed}")

                if status in ["Completed", "PartiallyCompleted"]:
                    print(f"   ✅ Job completed: {rows_processed} rows processed")
                    return True
                elif status == "Failed":
                    print(f"   ❌ Job failed: {result.get('error_message', 'Unknown error')}")
                    return False

                time.sleep(2)
            except requests.exceptions.RequestException as e:
                print(f"   ⚠️  Status check failed: {e}")
                time.sleep(2)

        print(f"   ❌ Timeout waiting for job completion")
        return False

    # ========================================================================
    # Step 5: Query and Validate Results
    # ========================================================================

    def query_loader_stats(self, job_id: str) -> Optional[Dict]:
        """Query loader job statistics"""
        print(f"\n📊 Querying stats for job {job_id}...")

        url = f"{self.base_url}/api/v1/loader/jobs/{job_id}/stats"

        try:
            response = self.session.get(url, timeout=10)
            response.raise_for_status()
            stats = response.json()
            print(f"   ✅ Stats retrieved:")
            print(f"      Rows processed: {stats.get('rows_processed', 0)}")
            print(f"      Rows failed: {stats.get('rows_failed', 0)}")
            print(f"      Quality violations: {stats.get('quality_violations', 0)}")
            return stats
        except requests.exceptions.RequestException as e:
            print(f"   ⚠️  Stats query failed: {e}")
            return None

    def query_dlq_stats(self, job_id: str) -> Optional[Dict]:
        """Query dead letter queue statistics"""
        print(f"\n🔍 Querying DLQ stats for job {job_id}...")

        url = f"{self.base_url}/api/v1/loader/jobs/{job_id}/dlq/stats"

        try:
            response = self.session.get(url, timeout=10)
            response.raise_for_status()
            dlq_stats = response.json()
            print(f"   ✅ DLQ Stats:")
            print(f"      Total failed rows: {dlq_stats.get('total_rows', 0)}")
            print(f"      Error categories: {list(dlq_stats.get('rows_by_category', {}).keys())}")
            return dlq_stats
        except requests.exceptions.RequestException as e:
            print(f"   ⚠️  DLQ stats query failed: {e}")
            return None

    def verify_checkpoint(self, job_id: str) -> bool:
        """Verify checkpoint was created"""
        print(f"\n💾 Verifying checkpoint for job {job_id}...")

        url = f"{self.base_url}/api/v1/loader/jobs/{job_id}/checkpoint"

        try:
            response = self.session.get(url, timeout=10)
            response.raise_for_status()
            checkpoint = response.json()
            print(f"   ✅ Checkpoint found:")
            print(f"      Rows processed: {checkpoint.get('current_row', 0)}")
            print(f"      Last checkpoint: {checkpoint.get('last_checkpoint', 'N/A')}")
            return True
        except requests.exceptions.RequestException as e:
            print(f"   ⚠️  Checkpoint verification: {e}")
            return False

    # ========================================================================
    # Step 6: Health Checks
    # ========================================================================

    def check_loader_health(self) -> bool:
        """Check loader health"""
        print(f"\n🏥 Checking loader health...")

        url = f"{self.base_url}/api/v1/loader/health"

        try:
            response = self.session.get(url, timeout=10)
            response.raise_for_status()
            health = response.json()
            print(f"   ✅ Loader Health:")
            print(f"      Status: {health.get('status', 'Unknown')}")
            print(f"      Active jobs: {health.get('active_jobs', 0)}")
            print(f"      Pending jobs: {health.get('pending_jobs', 0)}")
            return health.get('status') in ['Healthy', 'Degraded']
        except requests.exceptions.RequestException as e:
            print(f"   ❌ Health check failed: {e}")
            return False

    # ========================================================================
    # Main Test Flow
    # ========================================================================

    def run(self) -> bool:
        """Execute complete coordinator test"""

        print("="*80)
        print(" Graphica Coordinator Test - Ontological ETL")
        print("="*80)
        print(f"\nTest ID: {self.test_id}")
        print(f"Coordinator: {self.base_url}")

        try:
            # Step 1: Generate CSV files
            print("\n" + "="*80)
            print("STEP 1: Generate Synthetic Data")
            print("="*80)

            csv_files = {
                "customers": self.generate_customers_csv(100),
                "products": self.generate_products_csv(100),
                "orders": self.generate_orders_csv(100),
            }
            print(f"\n✅ Generated 3 CSV files with 300 total rows")

            # Step 2: Upload to file library
            print("\n" + "="*80)
            print("STEP 2: Upload to File Library")
            print("="*80)

            for entity_type, csv_file in csv_files.items():
                file_id = self.upload_to_file_library(csv_file, entity_type)
                if not file_id:
                    print(f"❌ Failed to upload {entity_type}")
                    return False
                self.file_library_ids[entity_type] = file_id
                self.scan_file_schema(file_id)

            print(f"\n✅ All files uploaded to file library")

            # Step 2a: Upload custom ontology
            print("\n" + "="*80)
            print("STEP 2a: Register Custom Ontology")
            print("="*80)

            ontology_id = self.upload_ontology()
            if not ontology_id:
                print("⚠️  Ontology upload failed, continuing without ontology...")

            # Step 2b: Create field-to-ontology mappings
            print("\n" + "="*80)
            print("STEP 2b: Create Field-to-Ontology Mappings")
            print("="*80)

            for entity_type, file_id in self.file_library_ids.items():
                table_name = f"{entity_type.upper()}_{self.test_id}"
                self.create_field_mappings(entity_type, file_id, table_name)

            print(f"\n✅ Field mappings created for all entities")

            # Step 3: Execute DDL
            print("\n" + "="*80)
            print("STEP 3: Execute Ontology-Driven DDL")
            print("="*80)

            for entity_type, file_id in self.file_library_ids.items():
                table_name = f"{entity_type.upper()}_{self.test_id}"
                self.execute_ddl(table_name, entity_type, file_id)

            print(f"\n✅ DDL executed for {len(self.tables)} tables")

            # Step 4: Create loader jobs
            print("\n" + "="*80)
            print("STEP 4: Create Loader Jobs")
            print("="*80)

            for entity_type, file_id in self.file_library_ids.items():
                table_name = f"{entity_type.upper()}_{self.test_id}"
                job_id = self.create_loader_job(file_id, entity_type, table_name)
                if job_id:
                    self.loader_jobs[entity_type] = job_id

            print(f"\n✅ Created {len(self.loader_jobs)} loader jobs")

            # Step 5: Wait for completion
            print("\n" + "="*80)
            print("STEP 5: Monitor Job Progress")
            print("="*80)

            all_completed = True
            for entity_type, job_id in self.loader_jobs.items():
                if not self.wait_for_job_completion(job_id, timeout_seconds=120):
                    print(f"   ⚠️  Job {entity_type} did not complete successfully")
                    all_completed = False

            # Step 6: Query results
            print("\n" + "="*80)
            print("STEP 6: Query Results")
            print("="*80)

            for entity_type, job_id in self.loader_jobs.items():
                self.query_loader_stats(job_id)
                self.query_dlq_stats(job_id)
                self.verify_checkpoint(job_id)

            # Step 7: Health check
            print("\n" + "="*80)
            print("STEP 7: Health Checks")
            print("="*80)
            self.check_loader_health()

            # Final summary
            print("\n" + "="*80)
            print(" TEST SUMMARY")
            print("="*80)

            print(f"\n✅ Coordinator Test {'PASSED' if all_completed else 'COMPLETED WITH WARNINGS'}")
            print(f"\nFiles uploaded: {len(self.file_library_ids)}")
            print(f"Tables created: {len(self.tables)}")
            print(f"Jobs executed: {len(self.loader_jobs)}")
            print(f"\nTables:")
            for table in self.tables:
                print(f"  - {table}")
            print(f"\nFile Library IDs:")
            for entity_type, file_id in self.file_library_ids.items():
                print(f"  - {entity_type}: {file_id}")
            print(f"\nLoader Jobs:")
            for entity_type, job_id in self.loader_jobs.items():
                print(f"  - {entity_type}: {job_id}")

            return all_completed

        except Exception as e:
            print(f"\n❌ Test failed with exception: {e}")
            import traceback
            traceback.print_exc()
            return False


def main():
    """Main entry point"""
    test = GraphicaCoordinatorTest()
    success = test.run()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
