-- Expand only: old relays ignore this nullable recovery token.
-- New roster writers set it atomically; existing members are not backfilled.
SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);

ALTER TABLE relay_members ADD COLUMN auto_join_pending UUID;
