SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);
SELECT set_config('statement_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('statement_timeout')::interval) * 1000, 0), 30000), 30000)::bigint::text || 'ms', true);

-- Retain old receipt intent and INSERT compatibility without a data rewrite.
ALTER TABLE community_auth_upgrades
    ADD COLUMN requested_requires_nip_fi BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN expected_policy_generation BIGINT,
    ADD COLUMN expected_requires_nip_fi BOOLEAN;
