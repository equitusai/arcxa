#!/usr/bin/env python3
"""
Comprehensive Customer360 Demo Data Generator
Generates synthetic data for all 34 tables across CRM and Transactions databases.
"""

import random
import json
from datetime import datetime, timedelta
from pathlib import Path
import uuid

# ============================================================================
# CONFIGURATION & CONSTANTS
# ============================================================================

# Sample data pools
FIRST_NAMES = ["James", "Mary", "John", "Patricia", "Robert", "Jennifer", "Michael", "Linda",
               "William", "Elizabeth", "David", "Barbara", "Richard", "Susan", "Joseph", "Jessica",
               "Thomas", "Sarah", "Charles", "Karen", "Christopher", "Nancy", "Daniel", "Lisa",
               "Matthew", "Betty", "Anthony", "Margaret", "Mark", "Sandra", "Donald", "Ashley",
               "Steven", "Kimberly", "Paul", "Emily", "Andrew", "Donna", "Joshua", "Michelle"]

LAST_NAMES = ["Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis",
              "Rodriguez", "Martinez", "Hernandez", "Lopez", "Gonzalez", "Wilson", "Anderson",
              "Thomas", "Taylor", "Moore", "Jackson", "Martin", "Lee", "Perez", "Thompson",
              "White", "Harris", "Sanchez", "Clark", "Ramirez", "Lewis", "Robinson", "Walker",
              "Young", "Allen", "King", "Wright", "Scott", "Torres", "Nguyen", "Hill", "Flores"]

CITIES = ["New York", "Los Angeles", "Chicago", "Houston", "Phoenix", "Philadelphia", "San Antonio",
          "San Diego", "Dallas", "San Jose", "Austin", "Jacksonville", "Fort Worth", "Columbus",
          "Charlotte", "San Francisco", "Indianapolis", "Seattle", "Denver", "Boston", "Nashville",
          "Detroit", "Portland", "Las Vegas", "Memphis", "Louisville", "Baltimore", "Milwaukee"]

STATES = ["AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN",
          "IA", "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV",
          "NH", "NJ", "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN",
          "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY"]

STREETS = ["Main St", "Oak Ave", "Maple Dr", "Park Blvd", "Washington St", "Lake Rd", "Hill St",
           "Cedar Ln", "Elm St", "Pine Ave", "Market St", "Church St", "Spring St", "Forest Dr"]

INDUSTRIES = ["Technology", "Healthcare", "Finance", "Retail", "Manufacturing", "Education",
              "Real Estate", "Automotive", "Telecommunications", "Energy", "Hospitality",
              "Construction", "Media", "Transportation", "Agriculture", "Pharmaceutical"]

PRODUCT_CATEGORIES_HIERARCHY = [
    {"name": "Electronics", "subcategories": ["Computers", "Smartphones", "Tablets", "Cameras", "Audio"]},
    {"name": "Clothing", "subcategories": ["Men's", "Women's", "Kids", "Shoes", "Accessories"]},
    {"name": "Home & Garden", "subcategories": ["Furniture", "Kitchen", "Bedding", "Decor", "Tools"]},
    {"name": "Sports & Outdoors", "subcategories": ["Fitness", "Camping", "Cycling", "Team Sports", "Water Sports"]},
    {"name": "Books & Media", "subcategories": ["Books", "Movies", "Music", "Games", "Software"]},
]

BRANDS = ["TechPro", "HomeStyle", "ActiveLife", "SmartChoice", "UrbanWear", "NatureCo",
          "PrecisionTools", "ComfortZone", "DigitalEdge", "ClassicCraft", "ModernLiving",
          "FitnessFirst", "EcoFriendly", "LuxuryLine", "BudgetBest", "PremiumSelect"]

# ============================================================================
# UTILITY FUNCTIONS
# ============================================================================

def random_date(start_days_ago, end_days_ago=0):
    """Generate random date within range."""
    days_ago = random.randint(end_days_ago, start_days_ago)
    return datetime.now() - timedelta(days=days_ago)

def random_phone():
    """Generate random US phone number."""
    if random.random() < 0.1:  # 10% null phones
        return None
    area = random.randint(200, 999)
    exchange = random.randint(200, 999)
    number = random.randint(1000, 9999)
    return f"({area}) {exchange}-{number}"

def random_email(first_name, last_name):
    """Generate email with occasional quality issues."""
    domains = ["gmail.com", "yahoo.com", "outlook.com", "example.com", "company.com"]
    patterns = [
        f"{first_name.lower()}{random.randint(1, 99)}@{random.choice(domains)}",
        f"{first_name[0].lower()}{last_name.lower()}@{random.choice(domains)}",
        f"{first_name.lower()}.{last_name.lower()}@{random.choice(domains)}",
        f"{first_name.lower()}_{last_name.lower()}@{random.choice(domains)}",
    ]
    email = random.choice(patterns)

    # Inject quality issues
    if random.random() < 0.05:  # 5% invalid emails
        email = email.replace("@", "..@")  # double dots

    return email

def generate_sku(product_index):
    """Generate product SKU."""
    return f"{random.choice(['EL', 'CL', 'HG', 'SP', 'BK'])}-{str(product_index).zfill(4)}-{random.choice(['A', 'B', 'C'])}"

# ============================================================================
# CUSTOMER DATA GENERATION (CRM DB)
# ============================================================================

def generate_customers(num_customers=1000, duplicate_rate=0.15):
    """Generate customers with intentional duplicates and quality issues."""
    print("  - Generating customers with duplicates and quality issues...")

    customers = []
    num_originals = int(num_customers * (1 - duplicate_rate))

    # Generate original customers
    for i in range(num_originals):
        customer_id = f"CUST{str(i+1).zfill(6)}"
        first_name = random.choice(FIRST_NAMES)
        last_name = random.choice(LAST_NAMES)

        customer = {
            "customer_id": customer_id,
            "first_name": first_name,
            "last_name": last_name,
            "email": random_email(first_name, last_name) if random.random() > 0.05 else None,
            "phone": random_phone(),
            "street": f"{random.randint(1, 9999)} {random.choice(STREETS)}",
            "city": random.choice(CITIES),
            "state": random.choice(STATES),
            "zip": f"{random.randint(10000, 99999)}",
            "registration_date": random_date(730, 30).date().isoformat(),
            "customer_segment": random.choice(["Bronze", "Silver", "Gold", "Platinum", "VIP"]),
            "loyalty_points": random.randint(0, 10000),
        }
        customers.append(customer)

    # Generate duplicates
    num_duplicates = num_customers - num_originals
    for i in range(num_duplicates):
        original = random.choice(customers[:num_originals])
        duplicate = create_duplicate_customer(original, random.choice([
            "name_typo", "nickname", "address_format", "phone_format", "email_variation"
        ]))
        duplicate["customer_id"] = f"CUST{str(num_originals + i + 1).zfill(6)}"
        customers.append(duplicate)

    random.shuffle(customers)
    return customers

def create_duplicate_customer(original, variation_type):
    """Create a duplicate customer with specific variation."""
    dup = original.copy()

    if variation_type == "name_typo":
        # Introduce typo in name
        if random.random() < 0.5 and len(dup["first_name"]) > 3:
            idx = random.randint(1, len(dup["first_name"]) - 2)
            dup["first_name"] = dup["first_name"][:idx] + dup["first_name"][idx+1:]
    elif variation_type == "nickname":
        # Use nickname
        nicknames = {"Robert": "Bob", "William": "Bill", "James": "Jim", "Richard": "Dick",
                    "Michael": "Mike", "Jennifer": "Jenny", "Elizabeth": "Beth"}
        if dup["first_name"] in nicknames:
            dup["first_name"] = nicknames[dup["first_name"]]
    elif variation_type == "address_format":
        # Same address, different format
        addr = random.choice([
            {"street": dup["street"].replace("St", "Street").replace("Ave", "Avenue")},
            {"street": dup["street"].upper()},
            {"street": dup["street"].lower()},
        ])
        dup["street"] = addr["street"]
        dup["city"] = addr.get("city", dup["city"])
        dup["state"] = addr.get("state", dup["state"])
        dup["zip"] = addr.get("zip", dup["zip"])
    elif variation_type == "phone_format":
        # Same phone, different format
        if dup["phone"] is not None:
            base = dup["phone"].replace("(", "").replace(")", "").replace("-", "").replace(".", "").replace("+1", "").replace(" ", "")
            if len(base) >= 10:
                base = base[:10]
                area, prefix, line = base[:3], base[3:6], base[6:10]
                formats = [
                    f"({area}) {prefix}-{line}",
                    f"{area}-{prefix}-{line}",
                    f"{area}.{prefix}.{line}",
                    f"{area}{prefix}{line}"
                ]
                dup["phone"] = random.choice(formats)
    elif variation_type == "email_variation":
        # Different email format but suggests same person
        dup["email"] = random_email(dup["first_name"], dup["last_name"])

    return dup

def generate_customer_accounts(customers, rate=0.3):
    """Generate B2B account hierarchies."""
    print("  - Generating customer accounts...")
    accounts = []
    account_id = 1

    for customer in customers:
        if random.random() < rate:
            account_type = random.choice(["personal", "business", "business", "enterprise"])
            account = {
                "account_id": f"ACC{str(account_id).zfill(6)}",
                "customer_id": customer["customer_id"],
                "account_name": f"{customer['first_name']} {customer['last_name']} {random.choice(['LLC', 'Inc', 'Corp', ''])}" if account_type != "personal" else f"{customer['first_name']} {customer['last_name']}",
                "account_type": account_type,
                "parent_account_id": None,  # Could link some accounts hierarchically
                "account_owner": f"{random.choice(FIRST_NAMES)} {random.choice(LAST_NAMES)}",
                "annual_revenue": round(random.uniform(10000, 10000000), 2) if account_type != "personal" else None,
                "employee_count": random.randint(1, 5000) if account_type != "personal" else None,
                "industry": random.choice(INDUSTRIES) if account_type != "personal" else None,
                "account_status": random.choice(["active", "active", "active", "inactive", "suspended"]),
            }
            accounts.append(account)
            account_id += 1

    return accounts

def generate_customer_addresses(customers):
    """Generate multiple addresses per customer."""
    print("  - Generating customer addresses...")
    addresses = []

    for customer in customers:
        # Each customer has 1-3 addresses
        num_addresses = random.choices([1, 2, 3], weights=[0.6, 0.3, 0.1])[0]
        address_types = ["billing", "shipping", "home", "work"]
        random.shuffle(address_types)

        for i in range(num_addresses):
            address = {
                "customer_id": customer["customer_id"],
                "address_type": address_types[i],
                "is_primary": i == 0,
                "street_line1": f"{random.randint(1, 9999)} {random.choice(STREETS)}",
                "street_line2": f"Apt {random.randint(1, 999)}" if random.random() < 0.3 else None,
                "city": random.choice(CITIES),
                "state": random.choice(STATES),
                "postal_code": f"{random.randint(10000, 99999)}",
                "country": "USA",
                "latitude": round(random.uniform(25.0, 49.0), 6),
                "longitude": round(random.uniform(-125.0, -65.0), 6),
                "verified": random.random() < 0.8,
                "verification_date": random_date(180).date().isoformat() if random.random() < 0.7 else None,
            }
            addresses.append(address)

    return addresses

def generate_customer_communications(customers):
    """Generate email/SMS campaign history."""
    print("  - Generating customer communications...")
    communications = []

    campaigns = [f"CAMP-{i}" for i in range(1, 21)]

    for customer in customers:
        # Each customer receives 0-10 communications
        num_comms = random.randint(0, 10)
        for _ in range(num_comms):
            sent_date = random_date(180)
            opened = random.random() < 0.4
            clicked = opened and random.random() < 0.3
            converted = clicked and random.random() < 0.2

            comm = {
                "customer_id": customer["customer_id"],
                "campaign_id": random.choice(campaigns),
                "communication_type": random.choice(["email", "email", "sms", "push"]),
                "subject": random.choice([
                    "Special Offer Just For You!",
                    "New Products You'll Love",
                    "Don't Miss Out - 20% Off Today",
                    "Your Weekly Newsletter",
                    "Important Account Update",
                ]),
                "sent_date": sent_date.isoformat(),
                "opened_date": (sent_date + timedelta(hours=random.randint(1, 48))).isoformat() if opened else None,
                "clicked_date": (sent_date + timedelta(hours=random.randint(1, 72))).isoformat() if clicked else None,
                "converted_date": (sent_date + timedelta(days=random.randint(1, 7))).isoformat() if converted else None,
                "bounced": random.random() < 0.02,
                "unsubscribed": random.random() < 0.01,
                "metadata": json.dumps({
                    "template_id": f"TPL-{random.randint(1, 50)}",
                    "ab_test_variant": random.choice(["A", "B", "control"]),
                    "personalization_score": round(random.uniform(0, 1), 2),
                }),
            }
            communications.append(comm)

    return communications

def generate_customer_segment_history(customers):
    """Generate temporal segment tracking."""
    print("  - Generating customer segment history...")
    history = []

    segments = ["Bronze", "Silver", "Gold", "Platinum", "VIP"]

    for customer in customers:
        # Generate 1-5 segment changes
        num_changes = random.randint(1, 5)
        current_date = datetime.strptime(customer["registration_date"], "%Y-%m-%d")

        for i in range(num_changes):
            end_date = current_date + timedelta(days=random.randint(30, 180))

            record = {
                "customer_id": customer["customer_id"],
                "segment_name": random.choice(segments),
                "segment_score": round(random.uniform(0, 100), 2),
                "effective_date": current_date.date().isoformat(),
                "end_date": end_date.date().isoformat() if i < num_changes - 1 else None,
                "reason": random.choice([
                    "Purchase threshold met",
                    "Inactivity period",
                    "High engagement score",
                    "Annual revenue target",
                    "Loyalty program promotion",
                ]),
            }
            history.append(record)
            current_date = end_date

    return history

def generate_customer_consent(customers):
    """Generate GDPR/privacy consent records."""
    print("  - Generating customer consent preferences...")
    consent_records = []

    consent_types = [
        "marketing_email",
        "marketing_sms",
        "analytics_tracking",
        "third_party_sharing",
        "personalized_advertising",
        "data_processing",
    ]

    for customer in customers:
        for consent_type in consent_types:
            consent_given = random.random() < 0.7
            consent_date = random_date(730)

            record = {
                "customer_id": customer["customer_id"],
                "consent_type": consent_type,
                "consent_given": consent_given,
                "consent_date": consent_date.isoformat(),
                "withdrawn_date": (consent_date + timedelta(days=random.randint(30, 500))).isoformat() if consent_given and random.random() < 0.1 else None,
                "consent_version": f"v{random.randint(1, 5)}.0",
                "ip_address": f"{random.randint(1, 255)}.{random.randint(0, 255)}.{random.randint(0, 255)}.{random.randint(1, 255)}",
                "user_agent": random.choice([
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
                    "Mozilla/5.0 (iPhone; CPU iPhone OS 14_6 like Mac OS X)",
                ]),
            }
            consent_records.append(record)

    return consent_records

def generate_loyalty_transactions(customers, transactions):
    """Generate loyalty points transactions."""
    print("  - Generating loyalty transactions...")
    loyalty_txns = []

    for customer in customers:
        # Generate 5-20 loyalty transactions per customer
        num_txns = random.randint(5, 20)
        for _ in range(num_txns):
            txn_type = random.choice(["earned", "earned", "earned", "redeemed", "expired", "adjusted"])

            txn = {
                "customer_id": customer["customer_id"],
                "transaction_type": txn_type,
                "points_amount": random.randint(10, 1000) if txn_type == "earned" else -random.randint(10, 500),
                "related_order_id": None,  # Could link to transaction_id
                "expiration_date": (datetime.now() + timedelta(days=random.randint(30, 730))).date().isoformat(),
                "transaction_date": random_date(365).isoformat(),
                "description": random.choice([
                    "Purchase reward",
                    "Birthday bonus",
                    "Referral bonus",
                    "Redeemed for discount",
                    "Points expired",
                    "Customer service adjustment",
                ]),
            }
            loyalty_txns.append(txn)

    return loyalty_txns

def generate_support_tickets(customers):
    """Generate support ticket records."""
    print("  - Generating support tickets...")
    tickets = []
    ticket_id = 1

    for customer in customers:
        # 20% of customers have tickets
        if random.random() < 0.2:
            num_tickets = random.randint(1, 5)
            for _ in range(num_tickets):
                opened_date = random_date(365)
                status = random.choice(["open", "in_progress", "waiting_customer", "resolved", "resolved", "closed"])

                first_response = opened_date + timedelta(hours=random.randint(1, 48)) if status != "open" else None
                resolved = opened_date + timedelta(days=random.randint(1, 14)) if status in ["resolved", "closed"] else None
                closed = resolved + timedelta(days=random.randint(1, 7)) if status == "closed" else None

                ticket = {
                    "ticket_id": f"TKT{str(ticket_id).zfill(8)}",
                    "customer_id": customer["customer_id"],
                    "subject": random.choice([
                        "Cannot login to account",
                        "Product not delivered",
                        "Billing question",
                        "Refund request",
                        "Technical issue with website",
                        "Product defect",
                    ]),
                    "description": "Customer reported an issue that needs attention.",
                    "priority": random.choice(["low", "medium", "medium", "high", "critical"]),
                    "status": status,
                    "category": random.choice(["Account", "Billing", "Technical", "Product", "Shipping", "General"]),
                    "assigned_to": f"{random.choice(FIRST_NAMES)} {random.choice(LAST_NAMES)}",
                    "opened_date": opened_date.isoformat(),
                    "first_response_date": first_response.isoformat() if first_response else None,
                    "resolved_date": resolved.isoformat() if resolved else None,
                    "closed_date": closed.isoformat() if closed else None,
                    "satisfaction_rating": random.randint(1, 5) if status in ["resolved", "closed"] else None,
                    "resolution_notes": "Issue resolved successfully." if status in ["resolved", "closed"] else None,
                }
                tickets.append(ticket)
                ticket_id += 1

    return tickets

def generate_customer_notes(customers):
    """Generate unstructured customer notes."""
    print("  - Generating customer notes...")
    notes = []

    for customer in customers:
        # 30% of customers have notes
        if random.random() < 0.3:
            num_notes = random.randint(1, 8)
            for _ in range(num_notes):
                note = {
                    "customer_id": customer["customer_id"],
                    "note_type": random.choice(["call", "meeting", "email", "general"]),
                    "note_text": random.choice([
                        "Customer called regarding shipping delay. Explained current situation.",
                        "Follow-up meeting scheduled for next week.",
                        "Customer interested in premium membership.",
                        "VIP customer - provide white-glove service.",
                        "Previous return due to sizing issue.",
                        "Customer prefers email communication.",
                    ]),
                    "author": f"{random.choice(FIRST_NAMES)} {random.choice(LAST_NAMES)}",
                    "is_pinned": random.random() < 0.1,
                    "visibility": random.choice(["internal", "internal", "customer_visible"]),
                    "note_date": random_date(365).isoformat(),
                }
                notes.append(note)

    return notes

def generate_tags_and_customer_tags(customers):
    """Generate tag taxonomy and customer tag associations."""
    print("  - Generating tags and customer tags...")

    # Create tag library
    tags = [
        {"tag_name": "high-value", "tag_category": "revenue"},
        {"tag_name": "at-risk", "tag_category": "churn"},
        {"tag_name": "new-customer", "tag_category": "lifecycle"},
        {"tag_name": "repeat-buyer", "tag_category": "behavior"},
        {"tag_name": "discount-seeker", "tag_category": "behavior"},
        {"tag_name": "brand-advocate", "tag_category": "engagement"},
        {"tag_name": "frequent-returner", "tag_category": "behavior"},
        {"tag_name": "mobile-preferred", "tag_category": "channel"},
        {"tag_name": "email-responsive", "tag_category": "channel"},
        {"tag_name": "price-sensitive", "tag_category": "segment"},
        {"tag_name": "tech-savvy", "tag_category": "segment"},
        {"tag_name": "seasonal-buyer", "tag_category": "pattern"},
    ]

    # Assign 0-5 tags per customer
    customer_tags = []
    for customer in customers:
        num_tags = random.randint(0, 5)
        selected_tags = random.sample(range(1, len(tags) + 1), min(num_tags, len(tags)))

        for tag_id in selected_tags:
            customer_tags.append({
                "customer_id": customer["customer_id"],
                "tag_id": tag_id,
                "tagged_by": f"{random.choice(FIRST_NAMES)} {random.choice(LAST_NAMES)}",
                "tagged_date": random_date(365).isoformat(),
            })

    return tags, customer_tags

def generate_customer_events(customers):
    """Generate behavioral event tracking."""
    print("  - Generating customer events...")
    events = []

    event_types = [
        "page_view", "product_view", "search", "add_to_cart",
        "remove_from_cart", "checkout_start", "checkout_complete",
        "account_login", "account_logout", "wishlist_add"
    ]

    pages = ["/", "/products", "/cart", "/checkout", "/account", "/deals", "/new-arrivals"]

    for customer in customers:
        # Each customer has 10-100 events
        num_events = random.randint(10, 100)
        session_id = str(uuid.uuid4())

        for _ in range(num_events):
            # New session every 10-20 events
            if random.random() < 0.1:
                session_id = str(uuid.uuid4())

            event = {
                "customer_id": customer["customer_id"],
                "event_type": random.choice(event_types),
                "event_timestamp": random_date(90).isoformat(),
                "session_id": session_id,
                "page_url": f"https://example.com{random.choice(pages)}",
                "referrer_url": random.choice([None, "https://google.com", "https://facebook.com", "https://example.com"]),
                "device_type": random.choice(["desktop", "mobile", "tablet"]),
                "browser": random.choice(["Chrome", "Firefox", "Safari", "Edge"]),
                "ip_address": f"{random.randint(1, 255)}.{random.randint(0, 255)}.{random.randint(0, 255)}.{random.randint(1, 255)}",
                "event_properties": json.dumps({
                    "product_id": f"PROD{random.randint(1, 500):04d}" if "product" in random.choice(event_types) else None,
                    "search_term": random.choice(["laptop", "shoes", "headphones", None]),
                    "value": round(random.uniform(10, 500), 2) if random.random() < 0.3 else None,
                }),
            }
            events.append(event)

    return events

def generate_customer_social_profiles(customers):
    """Generate social media profile connections."""
    print("  - Generating customer social profiles...")
    profiles = []

    platforms = ["facebook", "twitter", "linkedin", "instagram"]

    for customer in customers:
        # 40% of customers have connected social profiles
        if random.random() < 0.4:
            num_platforms = random.randint(1, 3)
            selected_platforms = random.sample(platforms, num_platforms)

            for platform in selected_platforms:
                profile = {
                    "customer_id": customer["customer_id"],
                    "platform": platform,
                    "profile_url": f"https://{platform}.com/{customer['first_name'].lower()}{customer['last_name'].lower()}{random.randint(1, 999)}",
                    "username": f"{customer['first_name'].lower()}{customer['last_name'][0].lower()}{random.randint(1, 999)}",
                    "follower_count": random.randint(10, 10000),
                    "verified": random.random() < 0.05,
                    "connected_date": random_date(365).date().isoformat(),
                    "last_sync_date": random_date(30).isoformat(),
                }
                profiles.append(profile)

    return profiles

def generate_customer_referrals(customers):
    """Generate customer referral records."""
    print("  - Generating customer referrals...")
    referrals = []

    for i, customer in enumerate(customers):
        # 10% of customers make referrals
        if random.random() < 0.1 and i < len(customers) - 10:
            num_referrals = random.randint(1, 3)
            for j in range(num_referrals):
                referred_idx = min(i + j + 1, len(customers) - 1)
                referral_date = random_date(365).date()
                converted = random.random() < 0.5

                referral = {
                    "referrer_customer_id": customer["customer_id"],
                    "referred_customer_id": customers[referred_idx]["customer_id"],
                    "referral_code": f"REF{customer['customer_id'][-4:]}{random.randint(100, 999)}",
                    "referral_date": referral_date.isoformat(),
                    "conversion_date": (referral_date + timedelta(days=random.randint(1, 30))).isoformat() if converted else None,
                    "reward_amount": 25.00 if converted else 0,
                    "reward_status": "paid" if converted else "pending",
                }
                referrals.append(referral)

    return referrals

# ============================================================================
# TRANSACTION DATA GENERATION (Transactions DB)
# ============================================================================

def generate_product_categories():
    """Generate hierarchical product categories."""
    print("  - Generating product categories...")
    categories = []
    category_id = 1

    for category in PRODUCT_CATEGORIES_HIERARCHY:
        # Parent category
        parent = {
            "category_id": category_id,
            "category_name": category["name"],
            "parent_category_id": None,
            "category_level": 1,
            "category_path": category["name"],
            "display_order": category_id,
            "is_active": True,
        }
        categories.append(parent)
        parent_id = category_id
        category_id += 1

        # Subcategories
        for subcat in category["subcategories"]:
            sub = {
                "category_id": category_id,
                "category_name": subcat,
                "parent_category_id": parent_id,
                "category_level": 2,
                "category_path": f"{category['name']} > {subcat}",
                "display_order": category_id,
                "is_active": True,
            }
            categories.append(sub)
            category_id += 1

    return categories

def generate_products(categories):
    """Generate comprehensive product catalog."""
    print("  - Generating products catalog...")
    products = []

    colors = ["Red", "Blue", "Green", "Black", "White", "Silver", "Gold", "Navy"]
    sizes = ["XS", "S", "M", "L", "XL", "XXL"]
    materials = ["Cotton", "Polyester", "Leather", "Metal", "Plastic", "Wood", "Glass"]

    for i in range(1, 501):  # 500 products
        product_id = f"PROD{str(i).zfill(4)}"
        category = random.choice([c for c in categories if c["category_level"] == 2])

        unit_price = round(random.uniform(9.99, 999.99), 2)
        cost_price = round(unit_price * random.uniform(0.4, 0.7), 2)
        msrp = round(unit_price * random.uniform(1.1, 1.5), 2)

        product = {
            "product_id": product_id,
            "product_name": f"{random.choice(BRANDS)} {category['category_name']} {random.choice(['Pro', 'Elite', 'Classic', 'Premium', 'Standard'])}",
            "product_description": f"High-quality {category['category_name'].lower()} with excellent features and durability.",
            "category_id": category["category_id"],
            "brand": random.choice(BRANDS),
            "sku": generate_sku(i),
            "barcode": f"{random.randint(100000000000, 999999999999)}",
            "unit_price": unit_price,
            "cost_price": cost_price,
            "msrp": msrp,
            "weight_kg": round(random.uniform(0.1, 25.0), 2),
            "dimensions_cm": f"{random.randint(10, 100)}x{random.randint(10, 100)}x{random.randint(5, 50)}",
            "color": random.choice(colors),
            "size": random.choice(sizes) if random.random() < 0.5 else None,
            "material": random.choice(materials),
            "is_active": random.random() < 0.95,
            "launch_date": random_date(730).date().isoformat(),
            "discontinued_date": random_date(30).date().isoformat() if random.random() < 0.05 else None,
            "attributes": json.dumps({
                "warranty_months": random.choice([6, 12, 24, 36]),
                "eco_friendly": random.choice([True, False]),
                "bestseller": random.random() < 0.1,
                "rating": round(random.uniform(3.0, 5.0), 1),
            }),
        }
        products.append(product)

    return products

def generate_warehouses():
    """Generate warehouse locations."""
    print("  - Generating warehouses...")
    warehouses = []

    warehouse_names = ["North Regional", "South Regional", "East Coast", "West Coast", "Central Hub"]

    for i, name in enumerate(warehouse_names):
        warehouse = {
            "warehouse_id": f"WH{str(i+1).zfill(3)}",
            "warehouse_name": name,
            "address": f"{random.randint(1, 9999)} Industrial Parkway",
            "city": random.choice(CITIES),
            "state": random.choice(STATES),
            "postal_code": f"{random.randint(10000, 99999)}",
            "country": "USA",
            "manager_name": f"{random.choice(FIRST_NAMES)} {random.choice(LAST_NAMES)}",
            "phone": random_phone(),
            "email": f"warehouse{i+1}@example.com",
            "capacity_cubic_meters": random.randint(5000, 50000),
            "is_active": True,
        }
        warehouses.append(warehouse)

    return warehouses

def generate_inventory_snapshots(products, warehouses):
    """Generate temporal inventory data."""
    print("  - Generating inventory snapshots...")
    snapshots = []

    # Generate daily snapshots for last 30 days
    for days_ago in range(30):
        snapshot_date = (datetime.now() - timedelta(days=days_ago)).date()

        # Snapshot for random subset of products
        sample_size = min(100, len(products))  # 100 products per day
        for product in random.sample(products, sample_size):
            for warehouse in random.sample(warehouses, random.randint(1, 3)):
                qty_on_hand = random.randint(0, 1000)
                qty_reserved = random.randint(0, min(qty_on_hand, 100))

                snapshot = {
                    "product_id": product["product_id"],
                    "warehouse_id": warehouse["warehouse_id"],
                    "snapshot_date": snapshot_date.isoformat(),
                    "quantity_on_hand": qty_on_hand,
                    "quantity_reserved": qty_reserved,
                    "quantity_available": qty_on_hand - qty_reserved,
                    "reorder_point": random.randint(10, 100),
                    "reorder_quantity": random.randint(50, 500),
                    "last_restock_date": random_date(60).date().isoformat(),
                    "unit_cost": product["cost_price"],
                }
                snapshots.append(snapshot)

    return snapshots

def generate_promotions():
    """Generate promotional campaigns."""
    print("  - Generating promotions...")
    promotions = []

    for i in range(1, 31):  # 30 promotions
        start_date = random_date(180).date()
        end_date = start_date + timedelta(days=random.randint(7, 60))

        promo = {
            "promotion_id": f"PROMO{str(i).zfill(4)}",
            "promotion_name": random.choice([
                "Summer Sale", "Holiday Special", "Back to School", "Black Friday",
                "Cyber Monday", "Spring Clearance", "Member Exclusive", "Flash Sale"
            ]) + f" {i}",
            "promotion_type": random.choice(["percentage_off", "fixed_amount", "bogo", "free_shipping"]),
            "discount_value": round(random.uniform(5.0, 50.0), 2),
            "start_date": start_date.isoformat(),
            "end_date": end_date.isoformat(),
            "promo_code": f"SAVE{random.randint(10, 50)}{random.choice(['NOW', 'TODAY', '2025'])}",
            "min_purchase_amount": round(random.uniform(25.0, 100.0), 2),
            "max_discount_amount": round(random.uniform(50.0, 200.0), 2),
            "usage_limit": random.randint(100, 10000),
            "usage_count": random.randint(0, 5000),
            "is_active": start_date <= datetime.now().date() <= end_date,
            "target_customer_segment": random.choice(["All", "Bronze", "Silver", "Gold", "Platinum", "VIP"]),
        }
        promotions.append(promo)

    return promotions

def generate_promotion_products(promotions, products):
    """Associate products with promotions."""
    print("  - Generating promotion-product associations...")
    associations = []

    for promo in promotions:
        # Each promotion applies to 5-50 products
        num_products = random.randint(5, 50)
        for product in random.sample(products, min(num_products, len(products))):
            associations.append({
                "promotion_id": promo["promotion_id"],
                "product_id": product["product_id"],
            })

    return associations

def generate_transactions(customers, products, num_transactions=5000):
    """Generate transaction records."""
    print("  - Generating transactions...")
    transactions = []

    statuses = ["completed", "completed", "completed", "pending", "shipped", "cancelled", "refunded"]
    payment_methods = ["credit_card", "debit_card", "paypal", "apple_pay", "google_pay"]

    for i in range(1, num_transactions + 1):
        customer = random.choice(customers)
        txn_date = random_date(365)

        # Random number of items (1-5)
        num_items = random.randint(1, 5)
        selected_products = random.sample(products, min(num_items, len(products)))
        amount = sum(float(p["unit_price"]) * random.randint(1, 3) for p in selected_products)

        txn = {
            "transaction_id": f"TXN{str(i).zfill(8)}",
            "customer_id": customer["customer_id"],
            "transaction_date": txn_date.isoformat(),
            "amount": round(amount, 2),
            "status": random.choice(statuses),
            "payment_method": random.choice(payment_methods),
        }
        transactions.append(txn)

    return transactions

def generate_transaction_items(transactions, products):
    """Generate line items for transactions."""
    print("  - Generating transaction items...")
    items = []

    for txn in transactions:
        num_items = random.randint(1, 5)
        for product in random.sample(products, min(num_items, len(products))):
            quantity = random.randint(1, 3)
            unit_price = float(product["unit_price"])
            discount = round(random.uniform(0, unit_price * 0.2), 2) if random.random() < 0.3 else 0

            item = {
                "transaction_id": txn["transaction_id"],
                "product_id": product["product_id"],
                "product_name": product["product_name"],
                "quantity": quantity,
                "unit_price": unit_price,
                "discount_amount": discount,
            }
            items.append(item)

    return items

def generate_payment_details(transactions):
    """Generate payment details."""
    print("  - Generating payment details...")
    payments = []

    card_types = ["Visa", "Mastercard", "Amex", "Discover"]
    processors = ["Stripe", "PayPal", "Square", "Authorize.net"]

    for txn in transactions:
        if txn["status"] in ["completed", "shipped"]:
            payment = {
                "transaction_id": txn["transaction_id"],
                "card_last_four": f"{random.randint(1000, 9999)}",
                "card_type": random.choice(card_types),
                "payment_processor": random.choice(processors),
                "authorization_code": f"AUTH{random.randint(100000, 999999)}",
            }
            payments.append(payment)

    return payments

def generate_shipping_info(transactions):
    """Generate shipping information."""
    print("  - Generating shipping info...")
    shipping = []

    methods = ["Standard", "Express", "Overnight", "Economy"]

    for txn in transactions:
        if txn["status"] in ["completed", "shipped"]:
            shipped_date = datetime.fromisoformat(txn["transaction_date"]) + timedelta(days=random.randint(1, 3))
            delivered_date = shipped_date + timedelta(days=random.randint(2, 7))

            ship = {
                "transaction_id": txn["transaction_id"],
                "shipping_address": f"{random.randint(1, 9999)} {random.choice(STREETS)}",
                "shipping_city": random.choice(CITIES),
                "shipping_state": random.choice(STATES),
                "shipping_zip": f"{random.randint(10000, 99999)}",
                "shipping_method": random.choice(methods),
                "tracking_number": f"1Z{random.randint(10000000, 99999999)}",
                "shipped_date": shipped_date.date().isoformat(),
                "delivered_date": delivered_date.date().isoformat() if random.random() < 0.9 else None,
            }
            shipping.append(ship)

    return shipping

def generate_transaction_refunds(transactions):
    """Generate refund records."""
    print("  - Generating refunds...")
    refunds = []
    refund_id = 1

    for txn in transactions:
        # 5% of transactions have refunds
        if random.random() < 0.05:
            requested_date = datetime.fromisoformat(txn["transaction_date"]) + timedelta(days=random.randint(1, 30))
            status = random.choice(["pending", "approved", "processed", "processed"])

            refund = {
                "refund_id": f"REF{str(refund_id).zfill(8)}",
                "transaction_id": txn["transaction_id"],
                "refund_amount": round(float(txn["amount"]) * random.uniform(0.5, 1.0), 2),
                "refund_reason": random.choice([
                    "Product defective",
                    "Wrong item received",
                    "Not as described",
                    "Customer changed mind",
                    "Item arrived too late",
                ]),
                "refund_type": random.choice(["full", "partial"]),
                "refund_status": status,
                "requested_date": requested_date.isoformat(),
                "processed_date": (requested_date + timedelta(days=random.randint(1, 7))).isoformat() if status == "processed" else None,
                "refund_method": txn["payment_method"],
                "notes": "Refund processed successfully" if status == "processed" else None,
            }
            refunds.append(refund)
            refund_id += 1

    return refunds

def generate_subscription_orders(customers):
    """Generate subscription records."""
    print("  - Generating subscriptions...")
    subscriptions = []
    sub_id = 1

    plans = [
        {"name": "Basic Monthly", "frequency": "monthly", "amount": 9.99},
        {"name": "Pro Monthly", "frequency": "monthly", "amount": 19.99},
        {"name": "Premium Annual", "frequency": "annual", "amount": 199.99},
    ]

    for customer in customers:
        # 15% of customers have subscriptions
        if random.random() < 0.15:
            plan = random.choice(plans)
            start_date = random_date(365).date()
            status = random.choice(["active", "active", "active", "paused", "cancelled"])

            sub = {
                "subscription_id": f"SUB{str(sub_id).zfill(6)}",
                "customer_id": customer["customer_id"],
                "subscription_plan": plan["name"],
                "billing_frequency": plan["frequency"],
                "subscription_amount": plan["amount"],
                "start_date": start_date.isoformat(),
                "next_billing_date": (start_date + timedelta(days=30 if plan["frequency"] == "monthly" else 365)).isoformat(),
                "end_date": (start_date + timedelta(days=random.randint(30, 730))).isoformat() if status in ["cancelled", "expired"] else None,
                "status": status,
                "payment_method_id": f"PM{random.randint(1000, 9999)}",
                "auto_renew": status == "active",
                "trial_end_date": (start_date + timedelta(days=14)).isoformat() if random.random() < 0.3 else None,
                "cancellation_reason": random.choice(["Too expensive", "No longer needed", "Poor service"]) if status == "cancelled" else None,
            }
            subscriptions.append(sub)
            sub_id += 1

    return subscriptions

def generate_cart_abandonments(customers, products):
    """Generate abandoned cart records."""
    print("  - Generating cart abandonments...")
    carts = []
    cart_id = 1

    for customer in customers:
        # 30% of customers have abandoned carts
        if random.random() < 0.3:
            num_carts = random.randint(1, 3)
            for _ in range(num_carts):
                created = random_date(60)
                num_items = random.randint(1, 5)
                selected = random.sample(products, min(num_items, len(products)))

                cart_items = []
                total_value = 0
                for product in selected:
                    qty = random.randint(1, 2)
                    price = float(product["unit_price"])
                    cart_items.append({
                        "product_id": product["product_id"],
                        "product_name": product["product_name"],
                        "quantity": qty,
                        "price": price,
                    })
                    total_value += price * qty

                cart = {
                    "cart_id": f"CART{str(cart_id).zfill(8)}",
                    "customer_id": customer["customer_id"],
                    "session_id": str(uuid.uuid4()),
                    "cart_created_date": created.isoformat(),
                    "last_activity_date": (created + timedelta(minutes=random.randint(5, 120))).isoformat(),
                    "cart_value": round(total_value, 2),
                    "item_count": len(cart_items),
                    "abandonment_reason": random.choice([
                        "High shipping cost",
                        "Looking for better price",
                        "Unexpected total",
                        "Distracted/interrupted",
                        None,
                    ]),
                    "recovery_email_sent": random.random() < 0.5,
                    "recovered_transaction_id": None,
                    "cart_items": json.dumps(cart_items),
                }
                carts.append(cart)
                cart_id += 1

    return carts

def generate_product_reviews(customers, transactions, products):
    """Generate product reviews."""
    print("  - Generating product reviews...")
    reviews = []

    for customer in customers:
        # 20% of customers leave reviews
        if random.random() < 0.2:
            num_reviews = random.randint(1, 5)
            for _ in range(num_reviews):
                product = random.choice(products)
                rating = random.choices([1, 2, 3, 4, 5], weights=[0.05, 0.05, 0.15, 0.35, 0.4])[0]

                review_texts = {
                    5: ["Excellent product! Highly recommend.", "Love it! Best purchase ever.", "5 stars! Perfect quality."],
                    4: ["Very good, minor issues.", "Great product overall.", "Happy with purchase."],
                    3: ["It's okay, meets expectations.", "Average product.", "Nothing special but works."],
                    2: ["Disappointed, not as described.", "Could be better.", "Has some issues."],
                    1: ["Terrible quality.", "Do not buy!", "Worst purchase ever."],
                }

                review = {
                    "product_id": product["product_id"],
                    "customer_id": customer["customer_id"],
                    "transaction_id": random.choice(transactions)["transaction_id"] if random.random() < 0.7 else None,
                    "rating": rating,
                    "review_title": f"{['Terrible', 'Poor', 'Average', 'Good', 'Excellent'][rating-1]} {product['product_name'][:20]}",
                    "review_text": random.choice(review_texts[rating]),
                    "is_verified_purchase": random.random() < 0.8,
                    "helpful_count": random.randint(0, 50),
                    "not_helpful_count": random.randint(0, 10),
                    "review_status": random.choice(["approved", "approved", "pending"]),
                    "reviewed_date": random_date(180).isoformat(),
                }
                reviews.append(review)

    return reviews

def generate_wishlists(customers, products):
    """Generate wishlist items."""
    print("  - Generating wishlists...")
    wishlists = []

    for customer in customers:
        # 40% of customers have wishlist items
        if random.random() < 0.4:
            num_items = random.randint(1, 10)
            for product in random.sample(products, min(num_items, len(products))):
                purchased = random.random() < 0.2
                added_date = random_date(180)

                wishlist = {
                    "customer_id": customer["customer_id"],
                    "product_id": product["product_id"],
                    "added_date": added_date.isoformat(),
                    "price_alert_threshold": round(float(product["unit_price"]) * 0.8, 2) if random.random() < 0.5 else None,
                    "is_purchased": purchased,
                    "purchased_date": (added_date + timedelta(days=random.randint(1, 60))).isoformat() if purchased else None,
                }
                wishlists.append(wishlist)

    return wishlists

def generate_product_price_history(products):
    """Generate temporal pricing data."""
    print("  - Generating product price history...")
    history = []

    for product in random.sample(products, min(200, len(products))):  # Price history for 200 products
        # Generate 2-5 price changes
        num_changes = random.randint(2, 5)
        current_date = datetime.strptime(product["launch_date"], "%Y-%m-%d")
        current_price = float(product["unit_price"])

        for i in range(num_changes):
            end_date = current_date + timedelta(days=random.randint(30, 90))

            record = {
                "product_id": product["product_id"],
                "price": round(current_price, 2),
                "effective_date": current_date.date().isoformat(),
                "end_date": end_date.date().isoformat() if i < num_changes - 1 else None,
                "reason": random.choice([
                    "seasonal", "promotion", "cost_change", "competitive", "inventory_clearance"
                ]),
            }
            history.append(record)

            # Next price is +/- 15%
            current_price *= random.uniform(0.85, 1.15)
            current_date = end_date

    return history

def generate_shipment_tracking(transactions):
    """Generate shipment tracking events."""
    print("  - Generating shipment tracking...")
    tracking_events = []

    event_sequence = [
        "picked_up",
        "in_transit",
        "in_transit",
        "out_for_delivery",
        "delivered",
    ]

    carriers = ["FedEx", "UPS", "USPS", "DHL"]

    for txn in transactions:
        if txn["status"] in ["completed", "shipped"]:
            tracking_number = f"1Z{random.randint(10000000, 99999999)}"
            current_time = datetime.fromisoformat(txn["transaction_date"]) + timedelta(days=1)

            for event_type in event_sequence:
                event = {
                    "tracking_number": tracking_number,
                    "transaction_id": txn["transaction_id"],
                    "event_type": event_type,
                    "event_location": f"{random.choice(CITIES)}, {random.choice(STATES)}",
                    "event_timestamp": current_time.isoformat(),
                    "carrier": random.choice(carriers),
                    "notes": f"Package {event_type.replace('_', ' ')}",
                }
                tracking_events.append(event)

                # Add some random exceptions
                if random.random() < 0.05:
                    exception_event = {
                        "tracking_number": tracking_number,
                        "transaction_id": txn["transaction_id"],
                        "event_type": "exception",
                        "event_location": event["event_location"],
                        "event_timestamp": (current_time + timedelta(hours=2)).isoformat(),
                        "carrier": event["carrier"],
                        "notes": random.choice([
                            "Weather delay",
                            "Address correction needed",
                            "Delivery attempted - recipient unavailable",
                        ]),
                    }
                    tracking_events.append(exception_event)

                current_time += timedelta(hours=random.randint(6, 24))

    return tracking_events

# ============================================================================
# MAIN GENERATION FUNCTION
# ============================================================================

def main():
    """Generate all demo data."""
    print("=" * 60)
    print("Comprehensive Customer360 Demo Data Generator")
    print("=" * 60)

    # Create output directories
    script_dir = Path(__file__).parent
    data_dir = script_dir / "data"
    parquet_dir = data_dir / "parquet"
    reference_dir = data_dir / "reference"

    parquet_dir.mkdir(parents=True, exist_ok=True)
    reference_dir.mkdir(parents=True, exist_ok=True)

    print("\n[1/3] Generating CRM data...")

    # Core customer data
    customers = generate_customers(num_customers=1000, duplicate_rate=0.15)

    # Extended customer data
    customer_accounts = generate_customer_accounts(customers)
    customer_addresses = generate_customer_addresses(customers)
    customer_communications = generate_customer_communications(customers)
    customer_segment_history = generate_customer_segment_history(customers)
    customer_consent = generate_customer_consent(customers)
    loyalty_transactions = generate_loyalty_transactions(customers, [])
    support_tickets = generate_support_tickets(customers)
    customer_notes = generate_customer_notes(customers)
    tags, customer_tags = generate_tags_and_customer_tags(customers)
    customer_events = generate_customer_events(customers)
    customer_social_profiles = generate_customer_social_profiles(customers)
    customer_referrals = generate_customer_referrals(customers)

    # Save CRM data
    print("\n[2/3] Generating Transactions data...")

    # Product catalog
    categories = generate_product_categories()
    products = generate_products(categories)
    warehouses = generate_warehouses()
    inventory_snapshots = generate_inventory_snapshots(products, warehouses)
    promotions = generate_promotions()
    promotion_products = generate_promotion_products(promotions, products)

    # Transactions
    transactions = generate_transactions(customers, products, num_transactions=5000)
    transaction_items = generate_transaction_items(transactions, products)
    payment_details = generate_payment_details(transactions)
    shipping_info = generate_shipping_info(transactions)
    transaction_refunds = generate_transaction_refunds(transactions)

    # Customer interactions with products
    subscription_orders = generate_subscription_orders(customers)
    cart_abandonments = generate_cart_abandonments(customers, products)
    product_reviews = generate_product_reviews(customers, transactions, products)
    wishlists = generate_wishlists(customers, products)
    product_price_history = generate_product_price_history(products)
    shipment_tracking = generate_shipment_tracking(transactions)

    print("\n[3/3] Writing data files...")

    # Write all CRM data
    with open(data_dir / "customers.json", "w") as f:
        json.dump(customers, f, indent=2)

    with open(data_dir / "customer_accounts.json", "w") as f:
        json.dump(customer_accounts, f, indent=2)

    with open(data_dir / "customer_addresses.json", "w") as f:
        json.dump(customer_addresses, f, indent=2)

    with open(data_dir / "customer_communications.json", "w") as f:
        json.dump(customer_communications, f, indent=2)

    with open(data_dir / "customer_segment_history.json", "w") as f:
        json.dump(customer_segment_history, f, indent=2)

    with open(data_dir / "customer_consent.json", "w") as f:
        json.dump(customer_consent, f, indent=2)

    with open(data_dir / "loyalty_transactions.json", "w") as f:
        json.dump(loyalty_transactions, f, indent=2)

    with open(data_dir / "support_tickets.json", "w") as f:
        json.dump(support_tickets, f, indent=2)

    with open(data_dir / "customer_notes.json", "w") as f:
        json.dump(customer_notes, f, indent=2)

    with open(data_dir / "tags.json", "w") as f:
        json.dump(tags, f, indent=2)

    with open(data_dir / "customer_tags.json", "w") as f:
        json.dump(customer_tags, f, indent=2)

    with open(data_dir / "customer_events.json", "w") as f:
        json.dump(customer_events, f, indent=2)

    with open(data_dir / "customer_social_profiles.json", "w") as f:
        json.dump(customer_social_profiles, f, indent=2)

    with open(data_dir / "customer_referrals.json", "w") as f:
        json.dump(customer_referrals, f, indent=2)

    # Write all Transactions data
    with open(data_dir / "product_categories.json", "w") as f:
        json.dump(categories, f, indent=2)

    with open(data_dir / "products.json", "w") as f:
        json.dump(products, f, indent=2)

    with open(data_dir / "warehouses.json", "w") as f:
        json.dump(warehouses, f, indent=2)

    with open(data_dir / "inventory_snapshots.json", "w") as f:
        json.dump(inventory_snapshots, f, indent=2)

    with open(data_dir / "promotions.json", "w") as f:
        json.dump(promotions, f, indent=2)

    with open(data_dir / "promotion_products.json", "w") as f:
        json.dump(promotion_products, f, indent=2)

    with open(data_dir / "transactions.json", "w") as f:
        json.dump(transactions, f, indent=2)

    with open(data_dir / "transaction_items.json", "w") as f:
        json.dump(transaction_items, f, indent=2)

    with open(data_dir / "payment_details.json", "w") as f:
        json.dump(payment_details, f, indent=2)

    with open(data_dir / "shipping_info.json", "w") as f:
        json.dump(shipping_info, f, indent=2)

    with open(data_dir / "transaction_refunds.json", "w") as f:
        json.dump(transaction_refunds, f, indent=2)

    with open(data_dir / "subscription_orders.json", "w") as f:
        json.dump(subscription_orders, f, indent=2)

    with open(data_dir / "cart_abandonments.json", "w") as f:
        json.dump(cart_abandonments, f, indent=2)

    with open(data_dir / "product_reviews.json", "w") as f:
        json.dump(product_reviews, f, indent=2)

    with open(data_dir / "wishlists.json", "w") as f:
        json.dump(wishlists, f, indent=2)

    with open(data_dir / "product_price_history.json", "w") as f:
        json.dump(product_price_history, f, indent=2)

    with open(data_dir / "shipment_tracking.json", "w") as f:
        json.dump(shipment_tracking, f, indent=2)

    # Summary stats
    print("\n" + "=" * 60)
    print("✓ Data generation complete!")
    print("=" * 60)
    print(f"\nCRM Database (16 tables):")
    print(f"  - {len(customers)} customers")
    print(f"  - {len(customer_accounts)} customer accounts")
    print(f"  - {len(customer_addresses)} addresses")
    print(f"  - {len(customer_communications)} communications")
    print(f"  - {len(customer_segment_history)} segment history records")
    print(f"  - {len(customer_consent)} consent records")
    print(f"  - {len(loyalty_transactions)} loyalty transactions")
    print(f"  - {len(support_tickets)} support tickets")
    print(f"  - {len(customer_notes)} customer notes")
    print(f"  - {len(tags)} tags, {len(customer_tags)} customer-tag associations")
    print(f"  - {len(customer_events)} customer events")
    print(f"  - {len(customer_social_profiles)} social profiles")
    print(f"  - {len(customer_referrals)} referrals")

    print(f"\nTransactions Database (18 tables):")
    print(f"  - {len(categories)} product categories")
    print(f"  - {len(products)} products")
    print(f"  - {len(warehouses)} warehouses")
    print(f"  - {len(inventory_snapshots)} inventory snapshots")
    print(f"  - {len(promotions)} promotions")
    print(f"  - {len(promotion_products)} promotion-product associations")
    print(f"  - {len(transactions)} transactions")
    print(f"  - {len(transaction_items)} transaction items")
    print(f"  - {len(payment_details)} payment records")
    print(f"  - {len(shipping_info)} shipping records")
    print(f"  - {len(transaction_refunds)} refunds")
    print(f"  - {len(subscription_orders)} subscriptions")
    print(f"  - {len(cart_abandonments)} abandoned carts")
    print(f"  - {len(product_reviews)} product reviews")
    print(f"  - {len(wishlists)} wishlist items")
    print(f"  - {len(product_price_history)} price history records")
    print(f"  - {len(shipment_tracking)} tracking events")

    total_records = sum([
        len(customers), len(customer_accounts), len(customer_addresses),
        len(customer_communications), len(customer_segment_history),
        len(customer_consent), len(loyalty_transactions), len(support_tickets),
        len(customer_notes), len(tags), len(customer_tags), len(customer_events),
        len(customer_social_profiles), len(customer_referrals),
        len(categories), len(products), len(warehouses), len(inventory_snapshots),
        len(promotions), len(promotion_products), len(transactions),
        len(transaction_items), len(payment_details), len(shipping_info),
        len(transaction_refunds), len(subscription_orders), len(cart_abandonments),
        len(product_reviews), len(wishlists), len(product_price_history),
        len(shipment_tracking)
    ])
    print(f"\nTotal records generated: {total_records:,}")
    print()

if __name__ == "__main__":
    main()
