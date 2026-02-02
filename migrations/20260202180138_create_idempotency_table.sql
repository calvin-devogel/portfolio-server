-- Add migration script here
CREATE TYPE header_pair AS (
    name TEXT,
    value BYTEA
);

CREATE TABLE idempotency (
    user_id uuid NOT NULL,
    idempotency_key TEXT NOT NULL,
    response_status_code SMALLINT,
    response_headers header_pair[],
    response_body BYTEA,
    created_at timestamptz NOT NULL,
    PRIMARY KEY(user_id, idempotency_key)
);