#!/usr/bin/env python3
"""
Streaming Data Generator

Continuously generates realistic updates to customer and transaction data
to simulate live data streaming through CDC.
"""

import os
import time
import random
import psycopg2
from datetime import datetime, timedelta
from typing import List, Dict

# Configuration
CRM_DB_CONFIG = {
    "host": os.getenv("CRM_DB_HOST", "crm-db"),
    "port": int(os.getenv("CRM_DB_PORT", 5432)),
    "database": "crm_db",
    "user": os.getenv("POSTGRES_USER", "graphica"),
    "password": os.getenv("POSTGRES_PASSWORD", "graphica_demo_2024")
}

TRANSACTIONS_DB_CONFIG = {
    "host": os.getenv("TRANSACTIONS_DB_HOST", "transactions-db"),
    "port": int(os.getenv("TRANSACTIONS_DB_PORT", 5432)),
    "database": "transactions_db",
    "user": os.getenv("POSTGRES_USER", "graphica"),
    "password": os.getenv("POSTGRES_PASSWORD", "graphica_demo_2024")
}

STREAM_INTERVAL = int(os.getenv("STREAM_INTERVAL_SECONDS", 5))
UPDATES_PER_INTERVAL = int(os.getenv("UPDATES_PER_INTERVAL", 3))


class StreamGenerator:
    """Generates streaming updates to demonstrate CDC."""

    def __init__(self):
        self.crm_conn = None
        self.transactions_conn = None
        self.customer_ids = []
        self.running = True

    def connect(self):
        """Connect to databases."""
        print("Connecting to databases...")

        max_retries = 30
        for i in range(max_retries):
            try:
                self.crm_conn = psycopg2.connect(**CRM_DB_CONFIG)
                self.transactions_conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
                print("✓ Connected to databases")
                break
            except psycopg2.OperationalError:
                print(f"  Waiting for databases... ({i+1}/{max_retries})")
                time.sleep(2)

        if not self.crm_conn or not self.transactions_conn:
            raise Exception("Failed to connect to databases")

    def load_customer_ids(self):
        """Load existing customer IDs."""
        cursor = self.crm_conn.cursor()
        cursor.execute("SELECT customer_id FROM customers")
        self.customer_ids = [row[0] for row in cursor.fetchall()]
        cursor.close()
        print(f"✓ Loaded {len(self.customer_ids)} customer IDs")

    def generate_customer_update(self):
        """Generate a random customer update."""
        if not self.customer_ids:
            return

        customer_id = random.choice(self.customer_ids)

        # Different types of updates
        update_type = random.choice([
            "loyalty_points",
            "customer_segment",
            "email",
            "phone",
            "address"
        ])

        cursor = self.crm_conn.cursor()

        if update_type == "loyalty_points":
            # Add loyalty points
            points = random.randint(10, 500)
            cursor.execute("""
                UPDATE customers
                SET loyalty_points = COALESCE(loyalty_points, 0) + %s,
                    updated_at = CURRENT_TIMESTAMP
                WHERE customer_id = %s
            """, (points, customer_id))
            print(f"  → Updated {customer_id}: +{points} loyalty points")

        elif update_type == "customer_segment":
            # Change customer segment
            segment = random.choice(["Premium", "Standard", "Basic"])
            cursor.execute("""
                UPDATE customers
                SET customer_segment = %s,
                    updated_at = CURRENT_TIMESTAMP
                WHERE customer_id = %s
            """, (segment, customer_id))
            print(f"  → Updated {customer_id}: segment = {segment}")

        elif update_type == "email":
            # Update email (simulate correction)
            cursor.execute("""
                UPDATE customers
                SET email = LOWER(first_name) || '.' || LOWER(last_name) || '@corrected.com',
                    updated_at = CURRENT_TIMESTAMP
                WHERE customer_id = %s
            """, (customer_id,))
            print(f"  → Updated {customer_id}: corrected email")

        elif update_type == "phone":
            # Update phone (simulate correction)
            area = random.randint(200, 999)
            prefix = random.randint(200, 999)
            line = random.randint(1000, 9999)
            phone = f"({area}) {prefix}-{line}"
            cursor.execute("""
                UPDATE customers
                SET phone = %s,
                    updated_at = CURRENT_TIMESTAMP
                WHERE customer_id = %s
            """, (phone, customer_id))
            print(f"  → Updated {customer_id}: phone = {phone}")

        elif update_type == "address":
            # Update address
            street_num = random.randint(100, 9999)
            cursor.execute("""
                UPDATE customers
                SET street = %s || ' Updated St',
                    updated_at = CURRENT_TIMESTAMP
                WHERE customer_id = %s
            """, (street_num, customer_id))
            print(f"  → Updated {customer_id}: address updated")

        self.crm_conn.commit()
        cursor.close()

    def generate_customer_interaction(self):
        """Generate a new customer interaction."""
        if not self.customer_ids:
            return

        customer_id = random.choice(self.customer_ids)

        interaction_types = [
            "phone_call", "email", "chat", "in_store_visit",
            "support_ticket", "feedback", "complaint", "inquiry"
        ]

        channels = ["phone", "email", "web", "mobile_app", "store"]

        notes = [
            "Customer inquired about product availability",
            "Resolved billing issue",
            "Provided account information",
            "Customer requested catalog",
            "Scheduled callback",
            "Answered shipping question",
            "Product recommendation provided"
        ]

        cursor = self.crm_conn.cursor()
        cursor.execute("""
            INSERT INTO customer_interactions (
                customer_id, interaction_type, interaction_date,
                channel, notes
            ) VALUES (%s, %s, %s, %s, %s)
        """, (
            customer_id,
            random.choice(interaction_types),
            datetime.now(),
            random.choice(channels),
            random.choice(notes)
        ))
        self.crm_conn.commit()
        cursor.close()
        print(f"  → New interaction for {customer_id}")

    def generate_new_transaction(self):
        """Generate a new transaction."""
        if not self.customer_ids:
            return

        customer_id = random.choice(self.customer_ids)

        # Generate transaction ID
        cursor = self.transactions_conn.cursor()
        cursor.execute("SELECT COUNT(*) FROM transactions")
        count = cursor.fetchone()[0]
        transaction_id = f"TXN{count + 1:08d}"

        amount = round(random.uniform(10.0, 1000.0), 2)
        status = random.choice(["completed", "completed", "completed", "pending"])
        payment_method = random.choice(["credit_card", "debit_card", "paypal", "apple_pay"])

        cursor.execute("""
            INSERT INTO transactions (
                transaction_id, customer_id, transaction_date,
                amount, status, payment_method
            ) VALUES (%s, %s, %s, %s, %s, %s)
        """, (
            transaction_id,
            customer_id,
            datetime.now(),
            amount,
            status,
            payment_method
        ))
        self.transactions_conn.commit()
        cursor.close()
        print(f"  → New transaction {transaction_id}: ${amount:.2f}")

    def generate_transaction_update(self):
        """Update an existing transaction status."""
        cursor = self.transactions_conn.cursor()

        # Find a pending transaction
        cursor.execute("""
            SELECT transaction_id FROM transactions
            WHERE status = 'pending'
            ORDER BY RANDOM()
            LIMIT 1
        """)

        result = cursor.fetchone()
        if result:
            transaction_id = result[0]
            new_status = random.choice(["completed", "cancelled"])

            cursor.execute("""
                UPDATE transactions
                SET status = %s,
                    updated_at = CURRENT_TIMESTAMP
                WHERE transaction_id = %s
            """, (new_status, transaction_id))
            self.transactions_conn.commit()
            print(f"  → Transaction {transaction_id}: {new_status}")

        cursor.close()

    def run_cycle(self):
        """Run one cycle of updates."""
        print(f"\n[{datetime.now().isoformat()}] Generating {UPDATES_PER_INTERVAL} updates...")

        for _ in range(UPDATES_PER_INTERVAL):
            action = random.choice([
                "customer_update",
                "customer_interaction",
                "new_transaction",
                "transaction_update"
            ])

            try:
                if action == "customer_update":
                    self.generate_customer_update()
                elif action == "customer_interaction":
                    self.generate_customer_interaction()
                elif action == "new_transaction":
                    self.generate_new_transaction()
                elif action == "transaction_update":
                    self.generate_transaction_update()
            except Exception as e:
                print(f"  Error generating {action}: {e}")

    def run(self):
        """Main streaming loop."""
        print("=" * 60)
        print("Graphica Streaming Data Generator")
        print("=" * 60)

        self.connect()
        self.load_customer_ids()

        print(f"\nStarting streaming updates (every {STREAM_INTERVAL}s)...")
        print("Press Ctrl+C to stop\n")

        cycle = 0
        try:
            while self.running:
                cycle += 1
                print(f"--- Cycle {cycle} ---")
                self.run_cycle()
                time.sleep(STREAM_INTERVAL)
        except KeyboardInterrupt:
            print("\n\nStopping stream generator...")
        finally:
            if self.crm_conn:
                self.crm_conn.close()
            if self.transactions_conn:
                self.transactions_conn.close()
            print("✓ Disconnected from databases")


if __name__ == "__main__":
    generator = StreamGenerator()
    generator.run()
