-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 1000), 1000)::bigint::text || 'ms', true);
-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('statement_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('statement_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);

ALTER TABLE artifact_heads ADD COLUMN IF NOT EXISTS modified_at TIMESTAMPTZ NOT NULL DEFAULT 'epoch';
ALTER TABLE artifact_heads ALTER COLUMN modified_at SET DEFAULT clock_timestamp();
ALTER TABLE artifact_revisions ADD COLUMN IF NOT EXISTS source_channel_id UUID;
ALTER TABLE artifact_revisions ADD COLUMN IF NOT EXISTS modified_at TIMESTAMPTZ NOT NULL DEFAULT 'epoch';
ALTER TABLE artifact_revisions ALTER COLUMN modified_at SET DEFAULT clock_timestamp();
ALTER TABLE channel_members ADD COLUMN IF NOT EXISTS sync_modified_at TIMESTAMPTZ NOT NULL DEFAULT 'epoch';
ALTER TABLE channel_members ALTER COLUMN sync_modified_at SET DEFAULT clock_timestamp();
ALTER TABLE communities ADD COLUMN IF NOT EXISTS artifact_policy_modified_at TIMESTAMPTZ NOT NULL DEFAULT 'epoch';
