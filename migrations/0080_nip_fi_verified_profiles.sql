-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);
-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('statement_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('statement_timeout')::interval) * 1000, 0), 30000), 30000)::bigint::text || 'ms', true);

CREATE TABLE nip_fi_profile_heads (
    community_id UUID NOT NULL,
    pubkey TEXT NOT NULL,
    issuer TEXT NOT NULL,
    issued_at NUMERIC(30, 0) NOT NULL,
    event JSONB NOT NULL,
    PRIMARY KEY (community_id, pubkey)
);
