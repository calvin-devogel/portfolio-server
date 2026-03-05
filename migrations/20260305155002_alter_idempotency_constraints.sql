-- Add migration script here
ALTER TABLE idempotency
    DROP CONSTRAINT IF EXISTS idempotency_pkey;

ALTER TABLE idempotency
    ADD COLUMN operation TEXT NOT NULL DEFAULT '';

CREATE UNIQUE INDEX idempotency_auth_unique
    ON idempotency (user_id, operation, idempotency_key)
    WHERE user_id IS NOT NULL;

CREATE UNIQUE INDEX idempotency_anon_unique
    ON idempotency (operation, idempotency_key)
    WHERE user_id IS NULL;