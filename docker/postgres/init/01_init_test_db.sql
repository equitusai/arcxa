-- Phase 2 Integration Test Database Initialization
-- This script sets up the test database for Graphica Phase 2 integration tests

-- Create extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Grant permissions
GRANT ALL PRIVILEGES ON DATABASE graphica_test TO postgres;

-- Create test schema
CREATE SCHEMA IF NOT EXISTS test_schema;

-- Sample test tables (will be created/dropped by tests dynamically)
-- These are just examples; actual tests will create their own tables

-- Performance tuning for test database
ALTER SYSTEM SET shared_buffers = '256MB';
ALTER SYSTEM SET effective_cache_size = '1GB';
ALTER SYSTEM SET maintenance_work_mem = '64MB';
ALTER SYSTEM SET checkpoint_completion_target = 0.9;
ALTER SYSTEM SET wal_buffers = '16MB';
ALTER SYSTEM SET default_statistics_target = 100;
ALTER SYSTEM SET random_page_cost = 1.1;
ALTER SYSTEM SET effective_io_concurrency = 200;
ALTER SYSTEM SET work_mem = '16MB';
ALTER SYSTEM SET min_wal_size = '1GB';
ALTER SYSTEM SET max_wal_size = '4GB';

-- Note: These settings require restart to take effect
-- For immediate effect, use SET commands in test sessions
