-- Add migration script here
ALTER TABLE users
    ADD COLUMN totp_secret TEXT,
    ADD COLUMN totp_enabled BOOLEAN NOT NULL DEFAULT FALSE;