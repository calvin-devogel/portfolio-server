-- Add migration script here
CREATE OR REPLACE FUNCTION cleanup_old_page_visits()
RETURNS void AS $$
BEGIN
    DELETE FROM page_visits WHERE visited_at < NOW() - INTERVAL '90 days';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION cleanup_old_performance_metrics()
RETURNS void AS $$
BEGIN
    DELETE FROM performance_metrics WHERE recorded_at < NOW() - INTERVAL '90 days';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION cleanup_old_server_metrics()
RETURNS void AS $$
BEGIN
    DELETE FROM server_metrics WHERE recorded_at < NOW() - INTERVAL '30 days';
END;
$$ LANGUAGE plpgsql;