-- CRM Database Initialization Script
-- Creates the customer relationship management database

CREATE DATABASE crm_db;

\c crm_db;

-- Create customers table
CREATE TABLE customers (
    customer_id VARCHAR(50) PRIMARY KEY,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    email VARCHAR(255),
    phone VARCHAR(50),
    street VARCHAR(255),
    city VARCHAR(100),
    state VARCHAR(50),
    zip VARCHAR(20),
    registration_date DATE,
    customer_segment VARCHAR(50),
    loyalty_points INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for common queries
CREATE INDEX idx_customers_email ON customers(email);
CREATE INDEX idx_customers_name ON customers(first_name, last_name);
CREATE INDEX idx_customers_phone ON customers(phone);
CREATE INDEX idx_customers_registration ON customers(registration_date);
CREATE INDEX idx_customers_segment ON customers(customer_segment);

-- Create customer interactions table
CREATE TABLE customer_interactions (
    interaction_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    interaction_type VARCHAR(50),
    interaction_date TIMESTAMP,
    channel VARCHAR(50),
    notes TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_interactions_customer ON customer_interactions(customer_id);
CREATE INDEX idx_interactions_date ON customer_interactions(interaction_date);

-- Create customer preferences table
CREATE TABLE customer_preferences (
    preference_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    preference_key VARCHAR(100),
    preference_value TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_preferences_customer ON customer_preferences(customer_id);

-- Create customer accounts table (B2B hierarchy)
CREATE TABLE customer_accounts (
    account_id VARCHAR(50) PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    account_name VARCHAR(255),
    account_type VARCHAR(50), -- personal, business, enterprise
    parent_account_id VARCHAR(50),
    account_owner VARCHAR(100),
    annual_revenue DECIMAL(15, 2),
    employee_count INTEGER,
    industry VARCHAR(100),
    account_status VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_accounts_customer ON customer_accounts(customer_id);
CREATE INDEX idx_accounts_parent ON customer_accounts(parent_account_id);
CREATE INDEX idx_accounts_type ON customer_accounts(account_type);

-- Create customer addresses table (multiple addresses)
CREATE TABLE customer_addresses (
    address_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    address_type VARCHAR(50), -- billing, shipping, home, work
    is_primary BOOLEAN DEFAULT false,
    street_line1 VARCHAR(255),
    street_line2 VARCHAR(255),
    city VARCHAR(100),
    state VARCHAR(50),
    postal_code VARCHAR(20),
    country VARCHAR(50) DEFAULT 'USA',
    latitude DECIMAL(10, 8),
    longitude DECIMAL(11, 8),
    verified BOOLEAN DEFAULT false,
    verification_date DATE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_addresses_customer ON customer_addresses(customer_id);
CREATE INDEX idx_addresses_type ON customer_addresses(address_type);
CREATE INDEX idx_addresses_primary ON customer_addresses(is_primary);

-- Create customer communications table (campaign history)
CREATE TABLE customer_communications (
    communication_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    campaign_id VARCHAR(50),
    communication_type VARCHAR(50), -- email, sms, push, direct_mail
    subject VARCHAR(500),
    sent_date TIMESTAMP,
    opened_date TIMESTAMP,
    clicked_date TIMESTAMP,
    converted_date TIMESTAMP,
    bounced BOOLEAN DEFAULT false,
    unsubscribed BOOLEAN DEFAULT false,
    metadata JSONB, -- flexible attributes
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_communications_customer ON customer_communications(customer_id);
CREATE INDEX idx_communications_campaign ON customer_communications(campaign_id);
CREATE INDEX idx_communications_type ON customer_communications(communication_type);
CREATE INDEX idx_communications_metadata ON customer_communications USING gin(metadata);

-- Create customer segments history (temporal tracking)
CREATE TABLE customer_segment_history (
    history_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    segment_name VARCHAR(100),
    segment_score DECIMAL(5, 2),
    effective_date DATE,
    end_date DATE,
    reason VARCHAR(255),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_segment_history_customer ON customer_segment_history(customer_id);
CREATE INDEX idx_segment_history_dates ON customer_segment_history(effective_date, end_date);

-- Create customer consent preferences (GDPR/privacy)
CREATE TABLE customer_consent (
    consent_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    consent_type VARCHAR(100), -- marketing_email, analytics, third_party_sharing, etc.
    consent_given BOOLEAN,
    consent_date TIMESTAMP,
    withdrawn_date TIMESTAMP,
    consent_version VARCHAR(20),
    ip_address VARCHAR(50),
    user_agent TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_consent_customer ON customer_consent(customer_id);
CREATE INDEX idx_consent_type ON customer_consent(consent_type);

-- Create loyalty transactions table
CREATE TABLE loyalty_transactions (
    loyalty_txn_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    transaction_type VARCHAR(50), -- earned, redeemed, expired, adjusted
    points_amount INTEGER,
    related_order_id VARCHAR(50),
    expiration_date DATE,
    transaction_date TIMESTAMP,
    description TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_loyalty_customer ON loyalty_transactions(customer_id);
CREATE INDEX idx_loyalty_type ON loyalty_transactions(transaction_type);
CREATE INDEX idx_loyalty_date ON loyalty_transactions(transaction_date);

-- Create support tickets table
CREATE TABLE support_tickets (
    ticket_id VARCHAR(50) PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    subject VARCHAR(500),
    description TEXT,
    priority VARCHAR(20), -- low, medium, high, critical
    status VARCHAR(50), -- open, in_progress, waiting_customer, resolved, closed
    category VARCHAR(100),
    assigned_to VARCHAR(100),
    opened_date TIMESTAMP,
    first_response_date TIMESTAMP,
    resolved_date TIMESTAMP,
    closed_date TIMESTAMP,
    satisfaction_rating INTEGER, -- 1-5
    resolution_notes TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tickets_customer ON support_tickets(customer_id);
CREATE INDEX idx_tickets_status ON support_tickets(status);
CREATE INDEX idx_tickets_priority ON support_tickets(priority);
CREATE INDEX idx_tickets_dates ON support_tickets(opened_date, resolved_date);

-- Create customer notes table (unstructured)
CREATE TABLE customer_notes (
    note_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    note_type VARCHAR(50), -- call, meeting, email, general
    note_text TEXT,
    author VARCHAR(100),
    is_pinned BOOLEAN DEFAULT false,
    visibility VARCHAR(20) DEFAULT 'internal', -- internal, customer_visible
    note_date TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_notes_customer ON customer_notes(customer_id);
CREATE INDEX idx_notes_type ON customer_notes(note_type);
CREATE INDEX idx_notes_date ON customer_notes(note_date);

-- Create customer tags table (many-to-many)
CREATE TABLE tags (
    tag_id SERIAL PRIMARY KEY,
    tag_name VARCHAR(100) UNIQUE,
    tag_category VARCHAR(100),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE customer_tags (
    customer_tag_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    tag_id INTEGER REFERENCES tags(tag_id),
    tagged_by VARCHAR(100),
    tagged_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_customer_tags_customer ON customer_tags(customer_id);
CREATE INDEX idx_customer_tags_tag ON customer_tags(tag_id);

-- Create customer events table (behavioral tracking)
CREATE TABLE customer_events (
    event_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    event_type VARCHAR(100), -- page_view, product_view, search, add_to_cart, etc.
    event_timestamp TIMESTAMP,
    session_id VARCHAR(100),
    page_url TEXT,
    referrer_url TEXT,
    device_type VARCHAR(50),
    browser VARCHAR(50),
    ip_address VARCHAR(50),
    event_properties JSONB,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_events_customer ON customer_events(customer_id);
CREATE INDEX idx_events_type ON customer_events(event_type);
CREATE INDEX idx_events_timestamp ON customer_events(event_timestamp);
CREATE INDEX idx_events_session ON customer_events(session_id);
CREATE INDEX idx_events_properties ON customer_events USING gin(event_properties);

-- Create customer social profiles table
CREATE TABLE customer_social_profiles (
    profile_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50) REFERENCES customers(customer_id),
    platform VARCHAR(50), -- facebook, twitter, linkedin, instagram
    profile_url TEXT,
    username VARCHAR(255),
    follower_count INTEGER,
    verified BOOLEAN DEFAULT false,
    connected_date DATE,
    last_sync_date TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_social_customer ON customer_social_profiles(customer_id);
CREATE INDEX idx_social_platform ON customer_social_profiles(platform);

-- Create customer referrals table
CREATE TABLE customer_referrals (
    referral_id SERIAL PRIMARY KEY,
    referrer_customer_id VARCHAR(50) REFERENCES customers(customer_id),
    referred_customer_id VARCHAR(50) REFERENCES customers(customer_id),
    referral_code VARCHAR(50),
    referral_date DATE,
    conversion_date DATE,
    reward_amount DECIMAL(10, 2),
    reward_status VARCHAR(50), -- pending, paid, expired
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_referrals_referrer ON customer_referrals(referrer_customer_id);
CREATE INDEX idx_referrals_referred ON customer_referrals(referred_customer_id);
CREATE INDEX idx_referrals_code ON customer_referrals(referral_code);

-- Grant permissions
GRANT ALL PRIVILEGES ON DATABASE crm_db TO graphica;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO graphica;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO graphica;
