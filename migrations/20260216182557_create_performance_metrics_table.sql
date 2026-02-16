-- Add migration script here
CREATE TABLE performance_metrics (
    metric_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    page_path VARCHAR(255) NOT NULL,
    metric_type VARCHAR(50) NOT NULL, -- FCP/LCP/CLS/FID/TTFB/page_load
    metric_value NUMERIC(10, 2) NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_performance_metrics_type ON performance_metrics(metric_type);
CREATE INDEX idx_performance_metrics_path ON performance_metrics(page_path);
CREATE INDEX idx_performance_metrics_recorded_at ON performance_metrics(recorded_at);