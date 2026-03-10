-- Graphica PostgreSQL initialization
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Fusion operations table
CREATE TABLE IF NOT EXISTS fusion_operations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fusion_id VARCHAR(255) UNIQUE NOT NULL,
    operation_type VARCHAR(50) NOT NULL,
    merged_entity_id VARCHAR(255) NOT NULL,
    confidence DOUBLE PRECISION,
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    reversed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_fusion_merged_entity ON fusion_operations(merged_entity_id);
CREATE INDEX idx_fusion_executed_at ON fusion_operations(executed_at);

-- Fusion participants table
CREATE TABLE IF NOT EXISTS fusion_participants (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fusion_id UUID REFERENCES fusion_operations(id) ON DELETE CASCADE,
    entity_id VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL, -- 'source' or 'merged'
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_fusion_participants_entity ON fusion_participants(entity_id);
CREATE INDEX idx_fusion_participants_fusion ON fusion_participants(fusion_id);

-- Fusion rules table
CREATE TABLE IF NOT EXISTS fusion_rules (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    rule_id VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    priority INTEGER DEFAULT 0,
    match_criteria JSONB NOT NULL,
    merge_strategy JSONB NOT NULL,
    min_confidence DOUBLE PRECISION DEFAULT 0.0,
    require_human_review BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_fusion_rules_priority ON fusion_rules(priority DESC);
