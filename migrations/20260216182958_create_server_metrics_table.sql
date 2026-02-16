-- Add migration script here
CREATE TABLE server_metrics (
    metric_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endpoint VARCHAR(255) NOT NULL,
    method VARCHAR(10) NOT NULL,
    status_code INTEGER NOT NULL,
    response_time_ms INTEGER NOT NULL,
    error_message TEXT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_server_metrics_endpoint ON server_metrics(endpoint);
CREATE INDEX idx_server_metrics_recorded_at ON server_metrics(recorded_at);
CREATE INDEX idx_server_metrics_status ON server_metrics(status_code);