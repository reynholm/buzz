-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);
-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('statement_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('statement_timeout')::interval) * 1000, 0), 30000), 30000)::bigint::text || 'ms', true);

-- Separate from the DDL transaction so validation does not block writers.
ALTER TABLE entity_deletion_requests
    VALIDATE CONSTRAINT entity_deletion_requests_status_check;
