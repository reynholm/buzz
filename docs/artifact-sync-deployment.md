# Incremental artifact reads deployment

Incremental artifact reads use a 120-second overlap and database modification
timestamps. The schema, indexes, and every serving writer must support
incremental reads.

## Schema and indexes

Migration `0078` adds columns with constant defaults, then changes the defaults
for future inserts. It does not rewrite existing rows or run a backfill.
Old rows remain at the epoch timestamp. An initial read includes them.
New application writers stamp every head change, grant, rejoin, removal,
redaction, and move source. Changes to channel listing also require recovery. Old application updates do not maintain all these
stamps, so mixed-version serving cannot advertise incremental correctness.

Relay startup migration and `buzz-admin migrate` build the indexes below
themselves within the migrations' budgets. A deployment too large for those
budgets runs with auto-migration off; before applying the migrations out of
band, first build the revision lookup index outside a transaction, before
migration `0077`:

```sql
CREATE INDEX CONCURRENTLY idx_artifact_revisions_artifact
    ON public.artifact_revisions (community_id, artifact_id);
```

Apply `0077_artifact_revision_lookup.sql` in a transaction after that prebuild.
Then apply `0078_artifact_sync_timestamps.sql` in a
transaction with its lock and statement budgets. The migration is idempotent
for this prebuild step. Then build these indexes outside a transaction:

```sql
CREATE INDEX CONCURRENTLY artifact_heads_modified
    ON public.artifact_heads (community_id, modified_at, artifact_id);
CREATE INDEX CONCURRENTLY artifact_heads_channel
    ON public.artifact_heads (community_id, channel_id, artifact_id);
CREATE INDEX CONCURRENTLY channel_members_sync
    ON public.channel_members (community_id, pubkey, sync_modified_at, channel_id);
CREATE INDEX CONCURRENTLY artifact_revisions_moves
    ON public.artifact_revisions (community_id, modified_at, event_id)
    WHERE source_channel_id IS NOT NULL;
```

Migration `0079` verifies each index definition and its valid, ready, and live
flags. Applied directly as SQL, it refuses a populated table whose index is
missing. A fresh empty
database can build the indexes under the migration's short budgets.
Migration `0077` also verifies the exact definition and all three flags.
Remove an invalid index with `DROP INDEX CONCURRENTLY` before a new build.

Relay startup migration and `buzz-admin migrate` build any of these indexes
that are missing before applying `0077` and `0079`. They use at most the migrations'
one-second lock and five-second statement budgets, retaining a tighter configured
migration policy. This is enough for a small
database. Each index builds in its own transaction. A database too large for that
budget fails closed with an error that points here. The migration whose index
failed is not recorded, while earlier migrations and indexes stay committed.
Prebuild the rest concurrently as above, then retry.

After all index builds succeed, run `buzz-admin migrate` with the target
`DATABASE_URL`. It verifies and records `0077`, reapplies the idempotent `0078`,
then verifies and records `0079`. Direct SQL application does not record the
migration ledger.

## Serving fleet

All instances must use the same configured relay signing key, because that key
authenticates the progress tokens. NIP-11 advertises `artifacts.incremental`.
Every serving writer must maintain the modification timestamps.

Drain every old HTTP, WebSocket, background and maintenance writer before
admitting the new serving fleet. There is no flag-off compatibility mode for
the old artifact writers. After the drain, clients must perform a new initial
sync before relying on incremental progress tokens; tokens issued by a mixed
fleet need another initial sync. Keep the same relay signing key across this
transition.

The 120-second stamp-to-visibility bound remains an operating assumption.
Runtime measurements are tracked separately.
