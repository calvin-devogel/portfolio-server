-- Add migration script here
CREATE TABLE message_rate_limits (
    email TEXT PRIMARY KEY,
    message_count INT NOT NULL DEFAULT 1,
    window_start timestamptz NOT NULL,
    last_message_at
);

-- check and update email rate limit
CREATE OR REPLACE FUNCTION check_email_rate_limit(
    p_email TEXT,
    p_max_messages INT,
    p_window_minutes INT
) RETURNS BOOLEAN AS $$
DECLARE
    v_count INT;
    v_window_start timestamptz;
BEGIN
    SELECT message_count, window_start INTO v_count, v_window_start
    FROM message_rate_limits
    WHERE email = p_email;

    IF NOT FOUND OR v_window_start < NOW() - (p_window_minutes || ' minutes')::INTERVAL THEN
        -- new window or email
        INSERT INTO message_rate_limits (email, message_count, window_start, last_message_at)
        VALUES (p_email, 1, NOW(), NOW())
        ON CONFLICT (email) DO UPDATE
        SET message_count = 1,
            window_start = NOW(),
            last_message_at = NOW();
        RETURN TRUE;
    ELSEIF v_count >= p_max_messages THEN
        RETURN FALSE;
    ELSE
        UPDATE message_rate_limits
        SET message_count = message_count + 1,
            last_message_at = NOW()
        WHERE email = p_email;
        RETURN TRUE;
    END IF;
END
$$ LANGUAGE plpgsql;