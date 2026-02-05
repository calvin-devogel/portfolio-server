-- Add migration script here
CREATE TABLE messages (
    message_id uuid NOT NULL,
    email TEXT NOT NULL,
    sender_name TEXT NOT NULL,
    message_text TEXT NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY(message_id)
);