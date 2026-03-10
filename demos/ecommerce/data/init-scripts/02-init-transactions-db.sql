-- Transactions Database Initialization Script
-- Creates the e-commerce transactions database

CREATE DATABASE transactions_db;

\c transactions_db;

-- Create transactions table
CREATE TABLE transactions (
    transaction_id VARCHAR(50) PRIMARY KEY,
    customer_id VARCHAR(50) NOT NULL,
    transaction_date TIMESTAMP NOT NULL,
    amount DECIMAL(10, 2),
    status VARCHAR(50),
    payment_method VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes
CREATE INDEX idx_transactions_customer ON transactions(customer_id);
CREATE INDEX idx_transactions_date ON transactions(transaction_date);
CREATE INDEX idx_transactions_status ON transactions(status);
CREATE INDEX idx_transactions_amount ON transactions(amount);

-- Create transaction items table
CREATE TABLE transaction_items (
    item_id SERIAL PRIMARY KEY,
    transaction_id VARCHAR(50) REFERENCES transactions(transaction_id),
    product_id VARCHAR(50),
    product_name VARCHAR(255),
    quantity INTEGER,
    unit_price DECIMAL(10, 2),
    discount_amount DECIMAL(10, 2) DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_transaction_items_transaction ON transaction_items(transaction_id);
CREATE INDEX idx_transaction_items_product ON transaction_items(product_id);

-- Create payment details table (for demonstration of sensitive data handling)
CREATE TABLE payment_details (
    payment_id SERIAL PRIMARY KEY,
    transaction_id VARCHAR(50) REFERENCES transactions(transaction_id),
    card_last_four VARCHAR(4),
    card_type VARCHAR(50),
    payment_processor VARCHAR(100),
    authorization_code VARCHAR(100),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_payment_details_transaction ON payment_details(transaction_id);

-- Create shipping information table
CREATE TABLE shipping_info (
    shipping_id SERIAL PRIMARY KEY,
    transaction_id VARCHAR(50) REFERENCES transactions(transaction_id),
    shipping_address TEXT,
    shipping_city VARCHAR(100),
    shipping_state VARCHAR(50),
    shipping_zip VARCHAR(20),
    shipping_method VARCHAR(50),
    tracking_number VARCHAR(100),
    shipped_date DATE,
    delivered_date DATE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_shipping_info_transaction ON shipping_info(transaction_id);

-- Create products table (catalog)
CREATE TABLE products (
    product_id VARCHAR(50) PRIMARY KEY,
    product_name VARCHAR(255),
    product_description TEXT,
    category_id INTEGER,
    brand VARCHAR(100),
    sku VARCHAR(100) UNIQUE,
    barcode VARCHAR(100),
    unit_price DECIMAL(10, 2),
    cost_price DECIMAL(10, 2),
    msrp DECIMAL(10, 2),
    weight_kg DECIMAL(8, 3),
    dimensions_cm VARCHAR(50), -- e.g., "30x20x10"
    color VARCHAR(50),
    size VARCHAR(20),
    material VARCHAR(100),
    is_active BOOLEAN DEFAULT true,
    launch_date DATE,
    discontinued_date DATE,
    attributes JSONB, -- flexible product attributes
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_brand ON products(brand);
CREATE INDEX idx_products_sku ON products(sku);
CREATE INDEX idx_products_active ON products(is_active);
CREATE INDEX idx_products_attributes ON products USING gin(attributes);

-- Create product categories table (hierarchical)
CREATE TABLE product_categories (
    category_id SERIAL PRIMARY KEY,
    category_name VARCHAR(255),
    parent_category_id INTEGER,
    category_level INTEGER,
    category_path VARCHAR(500), -- e.g., "Electronics > Computers > Laptops"
    display_order INTEGER,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_categories_parent ON product_categories(parent_category_id);
CREATE INDEX idx_categories_path ON product_categories(category_path);

-- Create inventory snapshots table (temporal inventory)
CREATE TABLE inventory_snapshots (
    snapshot_id SERIAL PRIMARY KEY,
    product_id VARCHAR(50) REFERENCES products(product_id),
    warehouse_id VARCHAR(50),
    snapshot_date DATE,
    quantity_on_hand INTEGER,
    quantity_reserved INTEGER,
    quantity_available INTEGER,
    reorder_point INTEGER,
    reorder_quantity INTEGER,
    last_restock_date DATE,
    unit_cost DECIMAL(10, 2),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_inventory_product ON inventory_snapshots(product_id);
CREATE INDEX idx_inventory_warehouse ON inventory_snapshots(warehouse_id);
CREATE INDEX idx_inventory_date ON inventory_snapshots(snapshot_date);

-- Create promotions table
CREATE TABLE promotions (
    promotion_id VARCHAR(50) PRIMARY KEY,
    promotion_name VARCHAR(255),
    promotion_type VARCHAR(50), -- percentage_off, fixed_amount, bogo, free_shipping
    discount_value DECIMAL(10, 2),
    start_date DATE,
    end_date DATE,
    promo_code VARCHAR(50),
    min_purchase_amount DECIMAL(10, 2),
    max_discount_amount DECIMAL(10, 2),
    usage_limit INTEGER,
    usage_count INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    target_customer_segment VARCHAR(100),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_promotions_code ON promotions(promo_code);
CREATE INDEX idx_promotions_dates ON promotions(start_date, end_date);
CREATE INDEX idx_promotions_active ON promotions(is_active);

-- Create promotion products table (many-to-many)
CREATE TABLE promotion_products (
    promo_product_id SERIAL PRIMARY KEY,
    promotion_id VARCHAR(50) REFERENCES promotions(promotion_id),
    product_id VARCHAR(50) REFERENCES products(product_id),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_promo_products_promotion ON promotion_products(promotion_id);
CREATE INDEX idx_promo_products_product ON promotion_products(product_id);

-- Create transaction refunds table
CREATE TABLE transaction_refunds (
    refund_id VARCHAR(50) PRIMARY KEY,
    transaction_id VARCHAR(50) REFERENCES transactions(transaction_id),
    refund_amount DECIMAL(10, 2),
    refund_reason VARCHAR(255),
    refund_type VARCHAR(50), -- full, partial
    refund_status VARCHAR(50), -- pending, approved, rejected, processed
    requested_date TIMESTAMP,
    processed_date TIMESTAMP,
    refund_method VARCHAR(50),
    notes TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_refunds_transaction ON transaction_refunds(transaction_id);
CREATE INDEX idx_refunds_status ON transaction_refunds(refund_status);
CREATE INDEX idx_refunds_date ON transaction_refunds(requested_date);

-- Create subscription orders table (recurring revenue)
CREATE TABLE subscription_orders (
    subscription_id VARCHAR(50) PRIMARY KEY,
    customer_id VARCHAR(50) NOT NULL,
    subscription_plan VARCHAR(100),
    billing_frequency VARCHAR(50), -- monthly, quarterly, annual
    subscription_amount DECIMAL(10, 2),
    start_date DATE,
    next_billing_date DATE,
    end_date DATE,
    status VARCHAR(50), -- active, paused, cancelled, expired
    payment_method_id VARCHAR(100),
    auto_renew BOOLEAN DEFAULT true,
    trial_end_date DATE,
    cancellation_reason VARCHAR(255),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_subscriptions_customer ON subscription_orders(customer_id);
CREATE INDEX idx_subscriptions_status ON subscription_orders(status);
CREATE INDEX idx_subscriptions_billing_date ON subscription_orders(next_billing_date);

-- Create cart abandonments table
CREATE TABLE cart_abandonments (
    cart_id VARCHAR(50) PRIMARY KEY,
    customer_id VARCHAR(50),
    session_id VARCHAR(100),
    cart_created_date TIMESTAMP,
    last_activity_date TIMESTAMP,
    cart_value DECIMAL(10, 2),
    item_count INTEGER,
    abandonment_reason VARCHAR(255),
    recovery_email_sent BOOLEAN DEFAULT false,
    recovered_transaction_id VARCHAR(50),
    cart_items JSONB, -- array of items in cart
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_carts_customer ON cart_abandonments(customer_id);
CREATE INDEX idx_carts_session ON cart_abandonments(session_id);
CREATE INDEX idx_carts_date ON cart_abandonments(cart_created_date);
CREATE INDEX idx_carts_items ON cart_abandonments USING gin(cart_items);

-- Create product reviews table
CREATE TABLE product_reviews (
    review_id SERIAL PRIMARY KEY,
    product_id VARCHAR(50) REFERENCES products(product_id),
    customer_id VARCHAR(50),
    transaction_id VARCHAR(50) REFERENCES transactions(transaction_id),
    rating INTEGER CHECK (rating >= 1 AND rating <= 5),
    review_title VARCHAR(255),
    review_text TEXT,
    is_verified_purchase BOOLEAN DEFAULT false,
    helpful_count INTEGER DEFAULT 0,
    not_helpful_count INTEGER DEFAULT 0,
    review_status VARCHAR(50) DEFAULT 'pending', -- pending, approved, rejected
    reviewed_date TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_reviews_product ON product_reviews(product_id);
CREATE INDEX idx_reviews_customer ON product_reviews(customer_id);
CREATE INDEX idx_reviews_rating ON product_reviews(rating);
CREATE INDEX idx_reviews_status ON product_reviews(review_status);

-- Create wishlist table
CREATE TABLE wishlists (
    wishlist_id SERIAL PRIMARY KEY,
    customer_id VARCHAR(50),
    product_id VARCHAR(50) REFERENCES products(product_id),
    added_date TIMESTAMP,
    price_alert_threshold DECIMAL(10, 2),
    is_purchased BOOLEAN DEFAULT false,
    purchased_date TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_wishlists_customer ON wishlists(customer_id);
CREATE INDEX idx_wishlists_product ON wishlists(product_id);
CREATE INDEX idx_wishlists_purchased ON wishlists(is_purchased);

-- Create product price history table (temporal pricing)
CREATE TABLE product_price_history (
    price_history_id SERIAL PRIMARY KEY,
    product_id VARCHAR(50) REFERENCES products(product_id),
    price DECIMAL(10, 2),
    effective_date DATE,
    end_date DATE,
    reason VARCHAR(255), -- seasonal, promotion, cost_change, competitive
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_price_history_product ON product_price_history(product_id);
CREATE INDEX idx_price_history_dates ON product_price_history(effective_date, end_date);

-- Create warehouses table
CREATE TABLE warehouses (
    warehouse_id VARCHAR(50) PRIMARY KEY,
    warehouse_name VARCHAR(255),
    address TEXT,
    city VARCHAR(100),
    state VARCHAR(50),
    postal_code VARCHAR(20),
    country VARCHAR(50),
    manager_name VARCHAR(100),
    phone VARCHAR(50),
    email VARCHAR(255),
    capacity_cubic_meters INTEGER,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_warehouses_active ON warehouses(is_active);

-- Create shipment tracking table
CREATE TABLE shipment_tracking (
    tracking_event_id SERIAL PRIMARY KEY,
    tracking_number VARCHAR(100),
    transaction_id VARCHAR(50) REFERENCES transactions(transaction_id),
    event_type VARCHAR(100), -- picked_up, in_transit, out_for_delivery, delivered, exception
    event_location VARCHAR(255),
    event_timestamp TIMESTAMP,
    carrier VARCHAR(100),
    notes TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tracking_number ON shipment_tracking(tracking_number);
CREATE INDEX idx_tracking_transaction ON shipment_tracking(transaction_id);
CREATE INDEX idx_tracking_timestamp ON shipment_tracking(event_timestamp);

-- Grant permissions
GRANT ALL PRIVILEGES ON DATABASE transactions_db TO graphica;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO graphica;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO graphica;
