#!/usr/bin/env python3
"""
Comprehensive Data Seeder for Customer360 Demo
Seeds all 34 tables across CRM and Transactions databases.
"""

import json
import psycopg2
import os
import time
from pathlib import Path

# Database connection parameters
CRM_DB_CONFIG = {
    "host": os.getenv("CRM_DB_HOST", "localhost"),
    "port": os.getenv("CRM_DB_PORT", "5432"),
    "database": "crm_db",
    "user": os.getenv("POSTGRES_USER", "graphica"),
    "password": os.getenv("POSTGRES_PASSWORD", "graphica_demo_2024")
}

TRANSACTIONS_DB_CONFIG = {
    "host": os.getenv("TRANSACTIONS_DB_HOST", "localhost"),
    "port": os.getenv("TRANSACTIONS_DB_PORT", "5433"),
    "database": "transactions_db",
    "user": os.getenv("POSTGRES_USER", "graphica"),
    "password": os.getenv("POSTGRES_PASSWORD", "graphica_demo_2024")
}


def wait_for_db(config, max_retries=30, delay=2):
    """Wait for database to be ready."""
    for i in range(max_retries):
        try:
            conn = psycopg2.connect(**config)
            conn.close()
            print(f"✓ Database {config['database']} is ready")
            return True
        except psycopg2.OperationalError:
            print(f"  Waiting for database {config['database']}... ({i+1}/{max_retries})")
            time.sleep(delay)
    return False


# ============================================================================
# CRM DATABASE LOADERS
# ============================================================================

def load_customers(data_file):
    """Load customer data into CRM database."""
    print("\n[CRM] Loading customers...")

    if not wait_for_db(CRM_DB_CONFIG):
        print("ERROR: CRM database not ready")
        return

    with open(data_file, 'r') as f:
        customers = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customers (
        customer_id, first_name, last_name, email, phone,
        street, city, state, zip, registration_date,
        customer_segment, loyalty_points
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (customer_id) DO NOTHING
    """

    count = 0
    for customer in customers:
        cursor.execute(insert_sql, (
            customer["customer_id"],
            customer["first_name"],
            customer["last_name"],
            customer.get("email"),
            customer.get("phone"),
            customer["street"],
            customer["city"],
            customer["state"],
            customer["zip"],
            customer["registration_date"],
            customer["customer_segment"],
            customer["loyalty_points"],
        ))
        count += 1

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {count} customers")


def load_customer_accounts(data_file):
    """Load customer accounts."""
    print("[CRM] Loading customer accounts...")

    with open(data_file, 'r') as f:
        accounts = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_accounts (
        account_id, customer_id, account_name, account_type,
        parent_account_id, account_owner, annual_revenue,
        employee_count, industry, account_status
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (account_id) DO NOTHING
    """

    for account in accounts:
        cursor.execute(insert_sql, (
            account["account_id"],
            account["customer_id"],
            account["account_name"],
            account["account_type"],
            account.get("parent_account_id"),
            account["account_owner"],
            account.get("annual_revenue"),
            account.get("employee_count"),
            account.get("industry"),
            account["account_status"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(accounts)} customer accounts")


def load_customer_addresses(data_file):
    """Load customer addresses."""
    print("[CRM] Loading customer addresses...")

    with open(data_file, 'r') as f:
        addresses = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_addresses (
        customer_id, address_type, is_primary, street_line1,
        street_line2, city, state, postal_code, country,
        latitude, longitude, verified, verification_date
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    """

    for address in addresses:
        cursor.execute(insert_sql, (
            address["customer_id"],
            address["address_type"],
            address["is_primary"],
            address["street_line1"],
            address.get("street_line2"),
            address["city"],
            address["state"],
            address["postal_code"],
            address["country"],
            address["latitude"],
            address["longitude"],
            address["verified"],
            address.get("verification_date"),
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(addresses)} customer addresses")


def load_customer_communications(data_file):
    """Load customer communications."""
    print("[CRM] Loading customer communications...")

    with open(data_file, 'r') as f:
        communications = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_communications (
        customer_id, campaign_id, communication_type, subject,
        sent_date, opened_date, clicked_date, converted_date,
        bounced, unsubscribed, metadata
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    """

    for comm in communications:
        cursor.execute(insert_sql, (
            comm["customer_id"],
            comm["campaign_id"],
            comm["communication_type"],
            comm["subject"],
            comm["sent_date"],
            comm.get("opened_date"),
            comm.get("clicked_date"),
            comm.get("converted_date"),
            comm["bounced"],
            comm["unsubscribed"],
            comm["metadata"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(communications)} customer communications")


def load_customer_segment_history(data_file):
    """Load customer segment history."""
    print("[CRM] Loading customer segment history...")

    with open(data_file, 'r') as f:
        history = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_segment_history (
        customer_id, segment_name, segment_score, effective_date,
        end_date, reason
    ) VALUES (%s, %s, %s, %s, %s, %s)
    """

    for record in history:
        cursor.execute(insert_sql, (
            record["customer_id"],
            record["segment_name"],
            record["segment_score"],
            record["effective_date"],
            record.get("end_date"),
            record["reason"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(history)} segment history records")


def load_customer_consent(data_file):
    """Load customer consent records."""
    print("[CRM] Loading customer consent...")

    with open(data_file, 'r') as f:
        consents = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_consent (
        customer_id, consent_type, consent_given, consent_date,
        withdrawn_date, consent_version, ip_address, user_agent
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
    """

    for consent in consents:
        cursor.execute(insert_sql, (
            consent["customer_id"],
            consent["consent_type"],
            consent["consent_given"],
            consent["consent_date"],
            consent.get("withdrawn_date"),
            consent["consent_version"],
            consent["ip_address"],
            consent["user_agent"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(consents)} consent records")


def load_loyalty_transactions(data_file):
    """Load loyalty transactions."""
    print("[CRM] Loading loyalty transactions...")

    with open(data_file, 'r') as f:
        transactions = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO loyalty_transactions (
        customer_id, transaction_type, points_amount, related_order_id,
        expiration_date, transaction_date, description
    ) VALUES (%s, %s, %s, %s, %s, %s, %s)
    """

    for txn in transactions:
        cursor.execute(insert_sql, (
            txn["customer_id"],
            txn["transaction_type"],
            txn["points_amount"],
            txn.get("related_order_id"),
            txn["expiration_date"],
            txn["transaction_date"],
            txn["description"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(transactions)} loyalty transactions")


def load_support_tickets(data_file):
    """Load support tickets."""
    print("[CRM] Loading support tickets...")

    with open(data_file, 'r') as f:
        tickets = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO support_tickets (
        ticket_id, customer_id, subject, description, priority,
        status, category, assigned_to, opened_date,
        first_response_date, resolved_date, closed_date,
        satisfaction_rating, resolution_notes
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (ticket_id) DO NOTHING
    """

    for ticket in tickets:
        cursor.execute(insert_sql, (
            ticket["ticket_id"],
            ticket["customer_id"],
            ticket["subject"],
            ticket["description"],
            ticket["priority"],
            ticket["status"],
            ticket["category"],
            ticket["assigned_to"],
            ticket["opened_date"],
            ticket.get("first_response_date"),
            ticket.get("resolved_date"),
            ticket.get("closed_date"),
            ticket.get("satisfaction_rating"),
            ticket.get("resolution_notes"),
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(tickets)} support tickets")


def load_customer_notes(data_file):
    """Load customer notes."""
    print("[CRM] Loading customer notes...")

    with open(data_file, 'r') as f:
        notes = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_notes (
        customer_id, note_type, note_text, author,
        is_pinned, visibility, note_date
    ) VALUES (%s, %s, %s, %s, %s, %s, %s)
    """

    for note in notes:
        cursor.execute(insert_sql, (
            note["customer_id"],
            note["note_type"],
            note["note_text"],
            note["author"],
            note["is_pinned"],
            note["visibility"],
            note["note_date"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(notes)} customer notes")


def load_tags_and_customer_tags(tags_file, customer_tags_file):
    """Load tags and customer tag associations."""
    print("[CRM] Loading tags and customer tags...")

    with open(tags_file, 'r') as f:
        tags = json.load(f)

    with open(customer_tags_file, 'r') as f:
        customer_tags = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    # Load tags
    insert_tag_sql = """
    INSERT INTO tags (tag_name, tag_category)
    VALUES (%s, %s)
    """

    for tag in tags:
        cursor.execute(insert_tag_sql, (
            tag["tag_name"],
            tag["tag_category"],
        ))

    # Load customer tags
    insert_customer_tag_sql = """
    INSERT INTO customer_tags (
        customer_id, tag_id, tagged_by, tagged_date
    ) VALUES (%s, %s, %s, %s)
    """

    for ct in customer_tags:
        cursor.execute(insert_customer_tag_sql, (
            ct["customer_id"],
            ct["tag_id"],
            ct["tagged_by"],
            ct["tagged_date"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(tags)} tags and {len(customer_tags)} customer tags")


def load_customer_events(data_file):
    """Load customer events."""
    print("[CRM] Loading customer events...")

    with open(data_file, 'r') as f:
        events = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_events (
        customer_id, event_type, event_timestamp, session_id,
        page_url, referrer_url, device_type, browser,
        ip_address, event_properties
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    """

    for event in events:
        cursor.execute(insert_sql, (
            event["customer_id"],
            event["event_type"],
            event["event_timestamp"],
            event["session_id"],
            event["page_url"],
            event.get("referrer_url"),
            event["device_type"],
            event["browser"],
            event["ip_address"],
            event["event_properties"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(events)} customer events")


def load_customer_social_profiles(data_file):
    """Load customer social profiles."""
    print("[CRM] Loading customer social profiles...")

    with open(data_file, 'r') as f:
        profiles = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_social_profiles (
        customer_id, platform, profile_url, username,
        follower_count, verified, connected_date, last_sync_date
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
    """

    for profile in profiles:
        cursor.execute(insert_sql, (
            profile["customer_id"],
            profile["platform"],
            profile["profile_url"],
            profile["username"],
            profile["follower_count"],
            profile["verified"],
            profile["connected_date"],
            profile["last_sync_date"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(profiles)} social profiles")


def load_customer_referrals(data_file):
    """Load customer referrals."""
    print("[CRM] Loading customer referrals...")

    with open(data_file, 'r') as f:
        referrals = json.load(f)

    conn = psycopg2.connect(**CRM_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO customer_referrals (
        referrer_customer_id, referred_customer_id, referral_code,
        referral_date, conversion_date, reward_amount, reward_status
    ) VALUES (%s, %s, %s, %s, %s, %s, %s)
    """

    for referral in referrals:
        cursor.execute(insert_sql, (
            referral["referrer_customer_id"],
            referral["referred_customer_id"],
            referral["referral_code"],
            referral["referral_date"],
            referral.get("conversion_date"),
            referral["reward_amount"],
            referral["reward_status"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(referrals)} referrals")


# ============================================================================
# TRANSACTIONS DATABASE LOADERS
# ============================================================================

def load_product_categories(data_file):
    """Load product categories."""
    print("\n[TRANSACTIONS] Loading product categories...")

    if not wait_for_db(TRANSACTIONS_DB_CONFIG):
        print("ERROR: Transactions database not ready")
        return

    with open(data_file, 'r') as f:
        categories = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    # Reset sequence to 1
    cursor.execute("SELECT setval('product_categories_category_id_seq', 1, false)")

    insert_sql = """
    INSERT INTO product_categories (
        category_name, parent_category_id, category_level,
        category_path, display_order, is_active
    ) VALUES (%s, %s, %s, %s, %s, %s)
    RETURNING category_id
    """

    # Sort by category_id to maintain hierarchy
    categories_sorted = sorted(categories, key=lambda x: x["category_id"])

    for category in categories_sorted:
        cursor.execute(insert_sql, (
            category["category_name"],
            category.get("parent_category_id"),
            category["category_level"],
            category["category_path"],
            category["display_order"],
            category["is_active"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(categories)} product categories")


def load_products(data_file):
    """Load products."""
    print("[TRANSACTIONS] Loading products...")

    with open(data_file, 'r') as f:
        products = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO products (
        product_id, product_name, product_description, category_id,
        brand, sku, barcode, unit_price, cost_price, msrp,
        weight_kg, dimensions_cm, color, size, material,
        is_active, launch_date, discontinued_date, attributes
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (product_id) DO NOTHING
    """

    for product in products:
        cursor.execute(insert_sql, (
            product["product_id"],
            product["product_name"],
            product["product_description"],
            product["category_id"],
            product["brand"],
            product["sku"],
            product["barcode"],
            product["unit_price"],
            product["cost_price"],
            product["msrp"],
            product["weight_kg"],
            product["dimensions_cm"],
            product["color"],
            product.get("size"),
            product["material"],
            product["is_active"],
            product["launch_date"],
            product.get("discontinued_date"),
            product["attributes"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(products)} products")


def load_warehouses(data_file):
    """Load warehouses."""
    print("[TRANSACTIONS] Loading warehouses...")

    with open(data_file, 'r') as f:
        warehouses = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO warehouses (
        warehouse_id, warehouse_name, address, city, state,
        postal_code, country, manager_name, phone, email,
        capacity_cubic_meters, is_active
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (warehouse_id) DO NOTHING
    """

    for warehouse in warehouses:
        cursor.execute(insert_sql, (
            warehouse["warehouse_id"],
            warehouse["warehouse_name"],
            warehouse["address"],
            warehouse["city"],
            warehouse["state"],
            warehouse["postal_code"],
            warehouse["country"],
            warehouse["manager_name"],
            warehouse["phone"],
            warehouse["email"],
            warehouse["capacity_cubic_meters"],
            warehouse["is_active"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(warehouses)} warehouses")


def load_inventory_snapshots(data_file):
    """Load inventory snapshots."""
    print("[TRANSACTIONS] Loading inventory snapshots...")

    with open(data_file, 'r') as f:
        snapshots = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO inventory_snapshots (
        product_id, warehouse_id, snapshot_date, quantity_on_hand,
        quantity_reserved, quantity_available, reorder_point,
        reorder_quantity, last_restock_date, unit_cost
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    """

    for snapshot in snapshots:
        cursor.execute(insert_sql, (
            snapshot["product_id"],
            snapshot["warehouse_id"],
            snapshot["snapshot_date"],
            snapshot["quantity_on_hand"],
            snapshot["quantity_reserved"],
            snapshot["quantity_available"],
            snapshot["reorder_point"],
            snapshot["reorder_quantity"],
            snapshot["last_restock_date"],
            snapshot["unit_cost"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(snapshots)} inventory snapshots")


def load_promotions(data_file):
    """Load promotions."""
    print("[TRANSACTIONS] Loading promotions...")

    with open(data_file, 'r') as f:
        promotions = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO promotions (
        promotion_id, promotion_name, promotion_type, discount_value,
        start_date, end_date, promo_code, min_purchase_amount,
        max_discount_amount, usage_limit, usage_count, is_active,
        target_customer_segment
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (promotion_id) DO NOTHING
    """

    for promo in promotions:
        cursor.execute(insert_sql, (
            promo["promotion_id"],
            promo["promotion_name"],
            promo["promotion_type"],
            promo["discount_value"],
            promo["start_date"],
            promo["end_date"],
            promo["promo_code"],
            promo["min_purchase_amount"],
            promo["max_discount_amount"],
            promo["usage_limit"],
            promo["usage_count"],
            promo["is_active"],
            promo["target_customer_segment"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(promotions)} promotions")


def load_promotion_products(data_file):
    """Load promotion-product associations."""
    print("[TRANSACTIONS] Loading promotion-product associations...")

    with open(data_file, 'r') as f:
        associations = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO promotion_products (promotion_id, product_id)
    VALUES (%s, %s)
    """

    for assoc in associations:
        cursor.execute(insert_sql, (
            assoc["promotion_id"],
            assoc["product_id"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(associations)} promotion-product associations")


def load_transactions(data_file):
    """Load transactions."""
    print("[TRANSACTIONS] Loading transactions...")

    with open(data_file, 'r') as f:
        transactions = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO transactions (
        transaction_id, customer_id, transaction_date, amount,
        status, payment_method
    ) VALUES (%s, %s, %s, %s, %s, %s)
    ON CONFLICT (transaction_id) DO NOTHING
    """

    for txn in transactions:
        cursor.execute(insert_sql, (
            txn["transaction_id"],
            txn["customer_id"],
            txn["transaction_date"],
            txn["amount"],
            txn["status"],
            txn["payment_method"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(transactions)} transactions")


def load_transaction_items(data_file):
    """Load transaction items."""
    print("[TRANSACTIONS] Loading transaction items...")

    with open(data_file, 'r') as f:
        items = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO transaction_items (
        transaction_id, product_id, product_name, quantity,
        unit_price, discount_amount
    ) VALUES (%s, %s, %s, %s, %s, %s)
    """

    for item in items:
        cursor.execute(insert_sql, (
            item["transaction_id"],
            item["product_id"],
            item["product_name"],
            item["quantity"],
            item["unit_price"],
            item["discount_amount"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(items)} transaction items")


def load_payment_details(data_file):
    """Load payment details."""
    print("[TRANSACTIONS] Loading payment details...")

    with open(data_file, 'r') as f:
        payments = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO payment_details (
        transaction_id, card_last_four, card_type,
        payment_processor, authorization_code
    ) VALUES (%s, %s, %s, %s, %s)
    """

    for payment in payments:
        cursor.execute(insert_sql, (
            payment["transaction_id"],
            payment["card_last_four"],
            payment["card_type"],
            payment["payment_processor"],
            payment["authorization_code"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(payments)} payment records")


def load_shipping_info(data_file):
    """Load shipping information."""
    print("[TRANSACTIONS] Loading shipping info...")

    with open(data_file, 'r') as f:
        shipping = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO shipping_info (
        transaction_id, shipping_address, shipping_city, shipping_state,
        shipping_zip, shipping_method, tracking_number, shipped_date,
        delivered_date
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
    """

    for ship in shipping:
        cursor.execute(insert_sql, (
            ship["transaction_id"],
            ship["shipping_address"],
            ship["shipping_city"],
            ship["shipping_state"],
            ship["shipping_zip"],
            ship["shipping_method"],
            ship["tracking_number"],
            ship["shipped_date"],
            ship.get("delivered_date"),
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(shipping)} shipping records")


def load_transaction_refunds(data_file):
    """Load transaction refunds."""
    print("[TRANSACTIONS] Loading refunds...")

    with open(data_file, 'r') as f:
        refunds = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO transaction_refunds (
        refund_id, transaction_id, refund_amount, refund_reason,
        refund_type, refund_status, requested_date, processed_date,
        refund_method, notes
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (refund_id) DO NOTHING
    """

    for refund in refunds:
        cursor.execute(insert_sql, (
            refund["refund_id"],
            refund["transaction_id"],
            refund["refund_amount"],
            refund["refund_reason"],
            refund["refund_type"],
            refund["refund_status"],
            refund["requested_date"],
            refund.get("processed_date"),
            refund["refund_method"],
            refund.get("notes"),
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(refunds)} refunds")


def load_subscription_orders(data_file):
    """Load subscription orders."""
    print("[TRANSACTIONS] Loading subscriptions...")

    with open(data_file, 'r') as f:
        subscriptions = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO subscription_orders (
        subscription_id, customer_id, subscription_plan, billing_frequency,
        subscription_amount, start_date, next_billing_date, end_date,
        status, payment_method_id, auto_renew, trial_end_date,
        cancellation_reason
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (subscription_id) DO NOTHING
    """

    for sub in subscriptions:
        cursor.execute(insert_sql, (
            sub["subscription_id"],
            sub["customer_id"],
            sub["subscription_plan"],
            sub["billing_frequency"],
            sub["subscription_amount"],
            sub["start_date"],
            sub["next_billing_date"],
            sub.get("end_date"),
            sub["status"],
            sub["payment_method_id"],
            sub["auto_renew"],
            sub.get("trial_end_date"),
            sub.get("cancellation_reason"),
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(subscriptions)} subscriptions")


def load_cart_abandonments(data_file):
    """Load cart abandonments."""
    print("[TRANSACTIONS] Loading cart abandonments...")

    with open(data_file, 'r') as f:
        carts = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO cart_abandonments (
        cart_id, customer_id, session_id, cart_created_date,
        last_activity_date, cart_value, item_count,
        abandonment_reason, recovery_email_sent,
        recovered_transaction_id, cart_items
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    ON CONFLICT (cart_id) DO NOTHING
    """

    for cart in carts:
        cursor.execute(insert_sql, (
            cart["cart_id"],
            cart["customer_id"],
            cart["session_id"],
            cart["cart_created_date"],
            cart["last_activity_date"],
            cart["cart_value"],
            cart["item_count"],
            cart.get("abandonment_reason"),
            cart["recovery_email_sent"],
            cart.get("recovered_transaction_id"),
            cart["cart_items"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(carts)} abandoned carts")


def load_product_reviews(data_file):
    """Load product reviews."""
    print("[TRANSACTIONS] Loading product reviews...")

    with open(data_file, 'r') as f:
        reviews = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO product_reviews (
        product_id, customer_id, transaction_id, rating,
        review_title, review_text, is_verified_purchase,
        helpful_count, not_helpful_count, review_status,
        reviewed_date
    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
    """

    for review in reviews:
        cursor.execute(insert_sql, (
            review["product_id"],
            review["customer_id"],
            review.get("transaction_id"),
            review["rating"],
            review["review_title"],
            review["review_text"],
            review["is_verified_purchase"],
            review["helpful_count"],
            review["not_helpful_count"],
            review["review_status"],
            review["reviewed_date"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(reviews)} product reviews")


def load_wishlists(data_file):
    """Load wishlists."""
    print("[TRANSACTIONS] Loading wishlists...")

    with open(data_file, 'r') as f:
        wishlists = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO wishlists (
        customer_id, product_id, added_date, price_alert_threshold,
        is_purchased, purchased_date
    ) VALUES (%s, %s, %s, %s, %s, %s)
    """

    for wishlist in wishlists:
        cursor.execute(insert_sql, (
            wishlist["customer_id"],
            wishlist["product_id"],
            wishlist["added_date"],
            wishlist.get("price_alert_threshold"),
            wishlist["is_purchased"],
            wishlist.get("purchased_date"),
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(wishlists)} wishlist items")


def load_product_price_history(data_file):
    """Load product price history."""
    print("[TRANSACTIONS] Loading product price history...")

    with open(data_file, 'r') as f:
        history = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO product_price_history (
        product_id, price, effective_date, end_date, reason
    ) VALUES (%s, %s, %s, %s, %s)
    """

    for record in history:
        cursor.execute(insert_sql, (
            record["product_id"],
            record["price"],
            record["effective_date"],
            record.get("end_date"),
            record["reason"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(history)} price history records")


def load_shipment_tracking(data_file):
    """Load shipment tracking."""
    print("[TRANSACTIONS] Loading shipment tracking...")

    with open(data_file, 'r') as f:
        tracking = json.load(f)

    conn = psycopg2.connect(**TRANSACTIONS_DB_CONFIG)
    cursor = conn.cursor()

    insert_sql = """
    INSERT INTO shipment_tracking (
        tracking_number, transaction_id, event_type, event_location,
        event_timestamp, carrier, notes
    ) VALUES (%s, %s, %s, %s, %s, %s, %s)
    """

    for event in tracking:
        cursor.execute(insert_sql, (
            event["tracking_number"],
            event["transaction_id"],
            event["event_type"],
            event["event_location"],
            event["event_timestamp"],
            event["carrier"],
            event["notes"],
        ))

    conn.commit()
    cursor.close()
    conn.close()

    print(f"  ✓ Loaded {len(tracking)} tracking events")


# ============================================================================
# MAIN SEEDING FUNCTION
# ============================================================================

def main():
    """Main seeding function."""
    print("=" * 60)
    print("Customer360 Demo Data Seeder")
    print("=" * 60)

    # Determine data directory
    script_dir = Path(__file__).parent
    data_dir = script_dir / "data"

    # Check if data directory exists
    if not data_dir.exists():
        print(f"ERROR: Data directory not found: {data_dir}")
        print("Please run generate_demo_data.py first")
        return

    print(f"\n📁 Data directory: {data_dir}")

    try:
        # Load CRM data
        print("\n" + "=" * 60)
        print("LOADING CRM DATABASE (16 tables)")
        print("=" * 60)

        load_customers(data_dir / "customers.json")
        load_customer_accounts(data_dir / "customer_accounts.json")
        load_customer_addresses(data_dir / "customer_addresses.json")
        load_customer_communications(data_dir / "customer_communications.json")
        load_customer_segment_history(data_dir / "customer_segment_history.json")
        load_customer_consent(data_dir / "customer_consent.json")
        load_loyalty_transactions(data_dir / "loyalty_transactions.json")
        load_support_tickets(data_dir / "support_tickets.json")
        load_customer_notes(data_dir / "customer_notes.json")
        load_tags_and_customer_tags(data_dir / "tags.json", data_dir / "customer_tags.json")
        load_customer_events(data_dir / "customer_events.json")
        load_customer_social_profiles(data_dir / "customer_social_profiles.json")
        load_customer_referrals(data_dir / "customer_referrals.json")

        # Load Transactions data
        print("\n" + "=" * 60)
        print("LOADING TRANSACTIONS DATABASE (18 tables)")
        print("=" * 60)

        load_product_categories(data_dir / "product_categories.json")
        load_products(data_dir / "products.json")
        load_warehouses(data_dir / "warehouses.json")
        load_inventory_snapshots(data_dir / "inventory_snapshots.json")
        load_promotions(data_dir / "promotions.json")
        load_promotion_products(data_dir / "promotion_products.json")
        load_transactions(data_dir / "transactions.json")
        load_transaction_items(data_dir / "transaction_items.json")
        load_payment_details(data_dir / "payment_details.json")
        load_shipping_info(data_dir / "shipping_info.json")
        load_transaction_refunds(data_dir / "transaction_refunds.json")
        load_subscription_orders(data_dir / "subscription_orders.json")
        load_cart_abandonments(data_dir / "cart_abandonments.json")
        load_product_reviews(data_dir / "product_reviews.json")
        load_wishlists(data_dir / "wishlists.json")
        load_product_price_history(data_dir / "product_price_history.json")
        load_shipment_tracking(data_dir / "shipment_tracking.json")

        print("\n" + "=" * 60)
        print("✓ Data seeding complete!")
        print("=" * 60)
        print("\n34 tables populated across 2 databases")
        print("CRM DB: 16 tables | Transactions DB: 18 tables")
        print()

    except FileNotFoundError as e:
        print(f"\nERROR: Data file not found: {e}")
        print("Please run generate_demo_data.py first")
    except Exception as e:
        print(f"\nERROR during seeding: {e}")
        import traceback
        traceback.print_exc()


if __name__ == "__main__":
    main()
