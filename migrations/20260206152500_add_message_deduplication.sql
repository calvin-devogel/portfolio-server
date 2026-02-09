-- Add migration script here
CREATE INDEX idx_messages_email_hash
    ON messages(email, md5(message_text));

-- check to prevent identical messages within a time window
CREATE OR REPLACE FUNCTION check_duplicate_message()
RETURNS TRIGGER AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM messages
        WHERE email = NEW.email
        AND md5(message_text) = md5(NEW.message_text)
        AND created_at > NOW() - INTERVAL '1 hour'
        AND message_id != NEW.message_id
    ) THEN
        RAISE EXCEPTION 'Duplicate message detected within 1 hour window';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER prevent_duplicate_messages
    BEFORE INSERT ON messages
    FOR EACH ROW
    EXECUTE FUNCTION check_duplicate_message();