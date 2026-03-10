-- Graphica TimescaleDB initialization
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- Attribute predictions time-series
CREATE TABLE IF NOT EXISTS attribute_predictions (
    time TIMESTAMPTZ NOT NULL,
    entity_id VARCHAR(255) NOT NULL,
    attribute_name VARCHAR(255) NOT NULL,
    value TEXT,
    confidence DOUBLE PRECISION,
    model_id VARCHAR(255),
    model_version VARCHAR(50),
    input_features JSONB,
    PRIMARY KEY (time, entity_id, attribute_name)
);

SELECT create_hypertable('attribute_predictions', 'time', if_not_exists => TRUE);

-- Retention policy: keep 2 years
SELECT add_retention_policy('attribute_predictions', INTERVAL '2 years', if_not_exists => TRUE);

-- Compression policy: compress data older than 30 days
SELECT add_compression_policy('attribute_predictions', INTERVAL '30 days', if_not_exists => TRUE);

-- Continuous aggregate: hourly summaries
CREATE MATERIALIZED VIEW IF NOT EXISTS attribute_predictions_hourly
WITH (timescaledb.continuous) AS
SELECT time_bucket('1 hour', time) AS bucket,
       entity_id,
       attribute_name,
       AVG(confidence) as avg_confidence,
       COUNT(*) as prediction_count
FROM attribute_predictions
GROUP BY bucket, entity_id, attribute_name;

-- Refresh policy for continuous aggregate
SELECT add_continuous_aggregate_policy('attribute_predictions_hourly',
    start_offset => INTERVAL '3 hours',
    end_offset => INTERVAL '1 hour',
    schedule_interval => INTERVAL '1 hour',
    if_not_exists => TRUE
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_attr_predictions_entity ON attribute_predictions(entity_id, time DESC);
CREATE INDEX IF NOT EXISTS idx_attr_predictions_model ON attribute_predictions(model_id, time DESC);
CREATE INDEX IF NOT EXISTS idx_attr_predictions_confidence ON attribute_predictions(confidence);
