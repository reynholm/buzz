-- Whether the relay joins every new community member into this channel.
-- Only a relay owner or admin sets it, with kind:9002 ["auto_join", "true"|
-- "false"]; it is accepted only on an open, unarchived stream channel.
-- Published in kind:39000 as a bare `auto_join` tag while it is effective
-- (set, open and unarchived), so what the event says is what the relay does.
-- The constant default fills existing rows without a table rewrite: no
-- channel is a default channel until someone asks for it.
-- Tighten the caller's budget; zero means unbounded.
SELECT set_config('lock_timeout',
    LEAST(COALESCE(NULLIF(EXTRACT(EPOCH FROM current_setting('lock_timeout')::interval) * 1000, 0), 5000), 5000)::bigint::text || 'ms', true);

ALTER TABLE channels
    ADD COLUMN auto_join BOOLEAN NOT NULL DEFAULT false;
