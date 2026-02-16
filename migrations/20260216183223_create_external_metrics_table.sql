-- Add migration script here
CREATE TABLE external_metrics (
    metric_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source VARCHAR(50) NOT NULL, -- CF/DO
    metric_name VARCHAR(100) NOT NULL,
    metric_value NUMERIC(15, 2) NOT NULL,
    metadata JSONB,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_external_metrics_source ON external_metrics(source);
CREATE INDEX idx_external_metrics_recorded_at ON external_metrics(recorded_at);