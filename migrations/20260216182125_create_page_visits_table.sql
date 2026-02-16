-- Add migration script here
CREATE TABLE page_visits (
    visit_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    page_path VARCHAR(255) NOT NULL,
    referrer_domain VARCHAR(255),
    visited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    session_hash VARCHAR(64) NOT NULL,
    duration_ms INTEGER
);

CREATE INDEX idx_page_visits_path ON page_visits(page_path);
CREATE INDEX idx_page_visits_visited_at ON page_visits(visited_at);
CREATE INDEX idx_page_visits_session_hash ON page_visits(session_hash);