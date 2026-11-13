-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);
-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('statement_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('statement_timeout')::interval) * 1000, 0), 30000), 30000)::bigint::text || 'ms', true);

-- Keep identity formats in the application, so public keys and account IDs
-- can share this target field. Requests remain restricted to the user entity type.
-- Existing demo requests remain valid.
ALTER TABLE entity_deletion_requests
    DROP CONSTRAINT entity_deletion_requests_target_check,
    ADD CONSTRAINT entity_deletion_requests_target_check
        CHECK (octet_length(target) >= 1 AND octet_length(target) <= 256 AND target = btrim(target)) NOT VALID;
