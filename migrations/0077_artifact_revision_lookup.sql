-- Populated databases prebuild this index concurrently.
-- See docs/artifact-sync-deployment.md.
-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 1000), 1000)::bigint::text || 'ms', true);
-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('statement_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('statement_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);

DO $$
BEGIN
    IF to_regclass('public.idx_artifact_revisions_artifact') IS NULL THEN
        IF EXISTS (SELECT 1 FROM public.artifact_revisions LIMIT 1) THEN
            RAISE EXCEPTION 'prebuild idx_artifact_revisions_artifact concurrently; see docs/artifact-sync-deployment.md';
        END IF;
        CREATE INDEX idx_artifact_revisions_artifact
            ON public.artifact_revisions (community_id, artifact_id);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_index
        WHERE indexrelid=to_regclass('public.idx_artifact_revisions_artifact')
          AND indisvalid AND indisready AND indislive
          AND pg_get_indexdef(indexrelid)=
              'CREATE INDEX idx_artifact_revisions_artifact ON public.artifact_revisions USING btree (community_id, artifact_id)'
    ) THEN
        RAISE EXCEPTION 'idx_artifact_revisions_artifact invalid or wrong definition; see docs/artifact-sync-deployment.md';
    END IF;
END $$;
