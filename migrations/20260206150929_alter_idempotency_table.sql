-- Add migration script here
-- don't need user id (sometimes)
ALTER TABLE idempotency
    ALTER COLUMN user_id DROP NOT NULL;

-- check to ensure at least one identifier exists
ALTER TABLE idempotency
    ADD CONSTRAINT idempotency_identifier_check
    CHECK (user_id IS NOT NULL OR idempotency_key IS NOT NULL);

-- add index for anonymous lookups
CREATE INDEX idx_idempotency_key_only
    ON idempotency(idempotency_key)
    WHERE user_id IS NULL;