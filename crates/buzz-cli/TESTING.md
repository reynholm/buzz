# buzz-cli Live Testing Guide

Manual testing runbook for verifying every CLI command against a local relay.
An agent or developer follows this step by step, running each command and
checking the output.

---

## 1. Prerequisites

Docker services running and healthy:

```bash
docker compose ps
# buzz-postgres   healthy
# buzz-redis      healthy
```

If not running: `just setup` from the repo root.

Tools: `jq`, `curl`, Rust toolchain.

---

## 2. Build the CLI

```bash
cargo build -p buzz-cli
```

Use `cargo run -p buzz-cli --` or the built binary at `target/debug/buzz`.

---

## 3. Start the Relay

In a separate terminal:

```bash
cd REPOS/buzz-nostr
set -a && source .env && set +a
cargo run -p buzz-relay
```

Verify:

```bash
curl -s http://localhost:3000/_liveness
# "ok" or 200 status
```

The `.env` should have `BUZZ_REQUIRE_AUTH_TOKEN=false` for local dev.

---

## 4. Mint Test Credentials

### Option A: buzz-admin (full scopes including admin)

This mints a token with all CLI-relevant scopes (including `admin:channels`)
via direct DB access. Use this for testing admin operations (archive,
delete-channel, add/remove-channel-member).

```bash
DATABASE_URL="${DATABASE_URL:?set DATABASE_URL for the local Buzz database}" \
cargo run -p buzz-admin -- mint-token \
  --name "cli-test" \
  --scopes "messages:read,messages:write,channels:read,channels:write,users:read,users:write,files:read,files:write,admin:channels"
```

This generates a keypair and prints:
- **Private key (nsec)** — save for `BUZZ_PRIVATE_KEY` testing

Export:

```bash
export BUZZ_RELAY_URL="http://localhost:3000"
export BUZZ_PRIVATE_KEY="nsec1..."   # from the mint output
```

### Scope reference

| Scope | Self-mintable | Needed for |
|-------|:---:|------------|
| `messages:read` | ✅ | `messages get`, `messages thread`, `messages search`, `feed get` |
| `messages:write` | ✅ | `messages send`, `messages edit`, `messages delete`, `reactions`, `messages vote` |
| `channels:read` | ✅ | `channels list`, `channels get`, `channels members` |
| `channels:write` | ✅ | `channels create`, `channels update`, `channels join`, `channels leave`, `channels topic`, `channels purpose` |
| `users:read` | ✅ | `users get`, `users presence` |
| `users:write` | ✅ | `users set-profile`, `users set-presence`, `users set-status` |
| `files:read` | ✅ | — |
| `files:write` | ✅ | — |
| `admin:channels` | ❌ | `channels archive`, `channels unarchive`, `channels delete`, `channels add-member`, `channels remove-member` |

---

## 5. Unit Tests

```bash
cargo test -p buzz-cli
# Expected: see cargo test -p buzz-cli for current count

cargo clippy -p buzz-cli -- -D warnings
# Expected: zero warnings
```

---

## 6. Live Testing — Command by Command

Run each command, verify exit code 0 and check output. Most commands
return JSON (pipe through `jq .` to validate). Commands are ordered so
earlier ones create resources that later ones need.

### 6.1 Channels

```bash
# channels create (stream)
buzz channels create --name "test-stream" --type stream --visibility open \
  --description "CLI test channel" | jq .
# Save the channel ID:
CHANNEL_ID=$(buzz channels create --name "test-cli" --type stream --visibility open | jq -r '.channel_id')
# Expected: {"event_id":"...","accepted":true,"message":"...","channel_id":"<uuid>"}

# channels create (forum) — needed for messages vote later
FORUM_ID=$(buzz channels create --name "test-forum" --type forum --visibility open | jq -r '.channel_id')

# channels list
buzz channels list | jq .
# Expected: [{"channel_id":"...","name":"...","description":"...","created_at":N}]
buzz channels list --visibility open | jq .
buzz channels list --member | jq .

# channels get
buzz channels get --channel "$CHANNEL_ID" | jq .
# Expected: {"channel_id":"...","name":"...","description":"...","created_at":N,"pubkey":"..."} or null

# channels update
buzz channels update --channel "$CHANNEL_ID" --name "test-cli-updated" \
  --description "Updated" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."}

# channels topic
buzz channels topic --channel "$CHANNEL_ID" --topic "Test topic" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."}

# channels purpose
buzz channels purpose --channel "$CHANNEL_ID" --purpose "Testing" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."}

# channels join (may already be a member from create)
buzz channels join --channel "$CHANNEL_ID" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."}

# channels leave
# NOTE: Fails with 400 "cannot remove the last owner" if this identity is the
# sole owner (which it is after channels create). To test leave successfully,
# first add-member a second pubkey as owner. The relay enforces ≥1 owner.
buzz channels leave --channel "$CHANNEL_ID" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."} (or 400 if last owner)

# Re-join so we can send messages
buzz channels join --channel "$CHANNEL_ID" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."}

# channels archive (requires admin:channels scope)
buzz channels archive --channel "$CHANNEL_ID" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."}

# channels unarchive
buzz channels unarchive --channel "$CHANNEL_ID" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"..."}
```

### 6.2 Canvas

```bash
# canvas set
buzz canvas set --channel "$CHANNEL_ID" --content "# Test Canvas" | jq .

# canvas set from stdin
echo "# Canvas from stdin" | buzz canvas set --channel "$CHANNEL_ID" --content - | jq .

# canvas get
buzz canvas get --channel "$CHANNEL_ID"
# Expected: raw markdown string, or: null
```

### 6.3 Messages

```bash
# messages send
MSG=$(buzz messages send --channel "$CHANNEL_ID" --content "Hello from CLI test" | jq .)
echo "$MSG"
EVENT_ID=$(echo "$MSG" | jq -r '.event_id')

# messages send with reply + broadcast
REPLY=$(buzz messages send --channel "$CHANNEL_ID" --content "Reply" \
  --reply-to "$EVENT_ID" --broadcast | jq .)
echo "$REPLY"
REPLY_ID=$(echo "$REPLY" | jq -r '.event_id')

# messages send with mentions — @name in content is auto-resolved, no flag needed
buzz messages send --channel "$CHANNEL_ID" --content "Hey @someone" | jq .

# messages send with NIP-27 nostr:npub1… inline mention — auto-resolved to p-tag
buzz messages send --channel "$CHANNEL_ID" \
  --content "Check with nostr:npub10elfcs4fr0l0r8af98jlmgdh9c8tcxjvz9qkw038js35mp4dma8qzvjptg on this" | jq .

# messages send from stdin — safe path for content with shell metacharacters
# (backticks, $vars, code blocks) that would otherwise be expanded by the shell.
echo 'Body with `backticks` and $vars stays literal.' \
  | buzz messages send --channel "$CHANNEL_ID" --content - | jq .

# Documents, archives, and Markdown can be attached together.
buzz messages send --channel "$CHANNEL_ID" --content "Attachments" \
  --file report.pdf --file bundle.zip --file notes.md | jq .
# Expected: three file cards with original basenames and byte sizes, not images.
# Click each card in Desktop and verify the saved name and bytes match the source.
# Generic files have a 100 MiB client cap; the relay may enforce a lower cap.

# Standalone upload also returns the original basename in `filename`.
buzz upload file --file notes.md | jq .
# Expected type: application/octet-stream for text without a magic signature.
# Download the returned URL with authenticated Blossom access and compare bytes:
buzz media get "$MEDIA_URL" --output downloaded-notes.md
cmp notes.md downloaded-notes.md

# Native download regression: upload a ZIP above 50 MiB and at most 100 MiB,
# then exercise the same Rust fetch used by FileCard before its save dialog.
# Supply the upload descriptor's URL and SHA-256, using the test relay identity.
BUZZ_TEST_ATTACHMENT_URL="$MEDIA_URL" BUZZ_TEST_ATTACHMENT_SHA256="$MEDIA_SHA256" \
  cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib \
  commands::media_download::file_download_tests::native_download_from_relay_matches_expected_sha256 \
  -- --ignored --exact
# Also send a multibyte filename above 255 UTF-8 bytes on a filesystem that
# supports it: imeta must fit 255 bytes, preserve the extension, and be accepted.

# messages get
buzz messages get --channel "$CHANNEL_ID" | jq .
buzz messages get --channel "$CHANNEL_ID" --limit 5 | jq .

# messages thread from the root, a reply, and a canonical link
buzz messages thread --channel "$CHANNEL_ID" --event "$EVENT_ID" | jq .
buzz messages thread --channel "$CHANNEL_ID" --event "$REPLY_ID" | jq .
buzz messages thread \
  --link "buzz://message?channel=$CHANNEL_ID&id=$REPLY_ID&thread=$EVENT_ID" | jq .

# messages search
buzz messages search --query "Hello" | jq .
buzz messages search --query "CLI test" --limit 5 | jq .

# messages edit
buzz messages edit --event "$EVENT_ID" --content "Edited by CLI test" | jq .

# messages delete
buzz messages delete --event "$REPLY_ID" | jq .
```

### 6.4 Diff Messages

```bash
# messages send-diff from stdin
echo '--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
-fn old() {}
+fn new() {}' | buzz messages send-diff \
  --channel "$CHANNEL_ID" \
  --diff - \
  --repo "https://github.com/example/repo" \
  --commit "abcdef1234567890abcdef1234567890abcdef12" | jq .

# messages send-diff with metadata
echo "diff content" | buzz messages send-diff \
  --channel "$CHANNEL_ID" \
  --diff - \
  --repo "https://github.com/example/repo" \
  --commit "abcdef1234567890abcdef1234567890abcdef12" \
  --file "src/main.rs" \
  --lang "rust" \
  --description "Refactored main" | jq .

# messages send-diff with branch + PR metadata
echo "diff content" | buzz messages send-diff \
  --channel "$CHANNEL_ID" \
  --diff - \
  --repo "https://github.com/example/repo" \
  --commit "abcdef1234567890abcdef1234567890abcdef12" \
  --parent-commit "1234567890abcdef1234567890abcdef12345678" \
  --source-branch "feature/cli" \
  --target-branch "main" \
  --pr 42 | jq .
```

### 6.5 Reactions

```bash
# Send a message to react to
REACT_MSG=$(buzz messages send --channel "$CHANNEL_ID" --content "React to this")
REACT_ID=$(echo "$REACT_MSG" | jq -r '.event_id')

# reactions add
buzz reactions add --event "$REACT_ID" --emoji "👍" | jq .

# reactions get
buzz reactions get --event "$REACT_ID" | jq .
# Expected: {"reactions":[{"emoji":"...","count":N,"pubkeys":["..."]}]}

# reactions remove
buzz reactions remove --event "$REACT_ID" --emoji "👍" | jq .
```

### 6.6 DMs

```bash
# dms list
buzz dms list | jq .
# Expected: [{"dm_id":"...","participants":["..."],"created_at":N}]

# dms open (needs a real pubkey — use your own or a test one)
# Get your own pubkey first:
MY_PUBKEY=$(buzz users get | jq -r '.[0].pubkey // empty')
echo "My pubkey: $MY_PUBKEY"

# dms open with a synthetic pubkey (relay will create the user)
DM_RESULT=$(buzz dms open --pubkey "0000000000000000000000000000000000000000000000000000000000000001")
echo "$DM_RESULT" | jq .
# Expected: {"event_id":"...","accepted":true,"message":"...","dm_id":"<uuid>"}
DM_ID=$(echo "$DM_RESULT" | jq -r '.dm_id')

# dms add-member (requires messages:write scope — NOT admin:channels)
buzz dms add-member --channel "$DM_ID" \
  --pubkey "0000000000000000000000000000000000000000000000000000000000000002" | jq .
```

### 6.7 Users & Presence

```bash
# users get — own profile (0 pubkeys)
buzz users get | jq .
# Expected: [{...profile...}] — always returns an array, even for single results

# users get — single pubkey
buzz users get --pubkey "$MY_PUBKEY" | jq .

# users get — batch (2+ pubkeys)
buzz users get --pubkey "$MY_PUBKEY" --pubkey "$MY_PUBKEY" | jq .

# users set-profile
buzz users set-profile --name "CLI Test Agent" --about "Testing buzz-cli" | jq .

# users presence
buzz users presence --pubkeys "$MY_PUBKEY" | jq .

# users set-presence
buzz users set-presence --status online | jq .
buzz users set-presence --status away | jq .
buzz users set-presence --status offline | jq .
# Note: set-presence may fail — kind:20001 is ephemeral and rejected by the HTTP bridge

# users set-status — NIP-38 kind:30315 on the d:general coordinate
buzz users set-status --text "reviewing PRs" --emoji "🔍" | jq .
buzz users set-status --text "no emoji this time" | jq .

# users set-status — emoji-only status (intentional: text is blank, emoji is kept)
buzz users set-status --text "" --emoji "🎶" | jq .

# users set-status --clear — removes the status (empty content, d:general only)
buzz users set-status --clear | jq .

# --clear is mutually exclusive with --text/--emoji
buzz users set-status --clear --text "nope" 2>&1; echo "exit: $?"
# Expected: exit 1 — clap conflict error
```

### 6.8 Channel Members (add/remove require admin:channels)

```bash
# channels add-member
buzz channels add-member --channel "$CHANNEL_ID" \
  --pubkey "0000000000000000000000000000000000000000000000000000000000000001" \
  --role member | jq .

# channels members
buzz channels members --channel "$CHANNEL_ID" | jq .
# Expected: [{"pubkey":"...","role":"..."}]

# channels remove-member
buzz channels remove-member --channel "$CHANNEL_ID" \
  --pubkey "0000000000000000000000000000000000000000000000000000000000000001" | jq .
```

### 6.9 Workflows

```bash
# workflows create
# NOTE: trigger uses `on:` tag (serde internally tagged enum).
# Valid triggers: message_posted, reaction_added, diff_posted, schedule, webhook
# Steps use `action:` tag: send_message, send_dm, set_channel_topic, add_reaction, etc.
WF=$(buzz workflows create --channel "$CHANNEL_ID" \
  --yaml 'name: test-wf
trigger:
  on: webhook
steps:
  - id: step1
    action: send_message
    text: "Hello from workflow"' | jq .)
echo "$WF"
WF_ID=$(echo "$WF" | jq -r '.workflow_id')

# workflows list
buzz workflows list --channel "$CHANNEL_ID" | jq .

# workflows get
buzz workflows get --workflow "$WF_ID" | jq .
# Expected: {"workflow_id":"...","content":"<yaml>","created_at":N,"pubkey":"..."} or null

# workflows update (requires --channel)
buzz workflows update --channel "$CHANNEL_ID" --workflow "$WF_ID" \
  --yaml 'name: test-wf-updated
trigger:
  on: webhook
steps:
  - id: step1
    action: send_message
    text: "Updated"' | jq .

# workflows trigger
# NOTE: May return 400 "workflow not found" — the relay indexes workflow
# definitions into a DB table asynchronously. If the definition event hasn't
# been indexed yet, the trigger handler won't find it.
buzz workflows trigger --workflow "$WF_ID" | jq .

# workflows runs
buzz workflows runs --workflow "$WF_ID" | jq .
# Expected: [] — relay stores runs in DB, not as Nostr events; empty is normal

# workflows approve — requires a workflow run waiting for approval
# This is hard to test ad-hoc without a workflow that has an approval gate.
# Test the validation instead:
buzz workflows approve --token "00000000-0000-0000-0000-000000000000" 2>&1 || true
# Should fail with relay error (token not found), not a validation error
# To test the deny path: buzz workflows approve --token <UUID> --approved false

# workflows delete
buzz workflows delete --workflow "$WF_ID" | jq .
```

### 6.10 Feed

```bash
buzz feed get | jq .
buzz feed get --limit 5 | jq .
# Expected: complete signed Nostr events with
# {id,pubkey,kind,content,created_at,sig,tags}, sorted newest-first
```

### 6.11 Forum & Voting

```bash
# Send a forum post (kind 45001) to the forum channel
FORUM_POST=$(buzz messages send --channel "$FORUM_ID" \
  --content "Forum post for vote testing" --kind 45001 | jq .)
echo "$FORUM_POST"
FORUM_EVENT_ID=$(echo "$FORUM_POST" | jq -r '.event_id')

# messages vote (up)
buzz messages vote --event "$FORUM_EVENT_ID" --direction up | jq .

# messages vote (down)
buzz messages vote --event "$FORUM_EVENT_ID" --direction down | jq .
```

### 6.12 Notes (NIP-23 long-form, kind:30023)

Editable team-knowledge notes keyed by `(kind:30023, you, d=slug)`. `set` is an
idempotent upsert; `rm` is a NIP-09 a-tag deletion. Output is plain text (refs),
not JSON — except `get`/`ls`, which emit JSON.

```bash
# set (first publish — --title required, body from stdin)
cat <<'EOF' | buzz notes set --name dco-check --title "DCO Check" \
  --summary "How we verify DCO" --tag dco --tag ci --content -
Run `git log --format='%(trailers:key=Signed-off-by)'` ...
EOF
# → prints event_id / naddr / coordinate / slug / title

# set (edit — omit --title to carry it forward; published_at preserved)
echo "Updated body." | buzz notes set --name dco-check --content -

# get by name (own author resolves directly; cross-author #d query otherwise)
buzz notes get --name dco-check | jq .
buzz notes get --name dco-check --content-only

# get by naddr (exact coordinate; paste the naddr from a set/get above)
buzz notes get --naddr "$NADDR" | jq .

# ls (own by default; --author all across the team; --tag filters)
buzz notes ls | jq .
buzz notes ls --tag dco | jq .
buzz notes ls --author all --limit 10 | jq .

# rm (NIP-09 a-tag deletion; subsequent get must 404)
buzz notes rm --name dco-check
# → prints deleted <coordinate> / deletion <event-id>
buzz notes get --name dco-check   # exits non-zero: not found

# rm of a slug you never published → NotFound, no kind:5 emitted
buzz notes rm --name does-not-exist   # exits non-zero
```

---

## 7. Error Path Testing

Verify the CLI produces correct JSON on stderr and correct exit codes.

```bash
# Exit 1: Invalid UUID
buzz channels get --channel "not-a-uuid" 2>&1; echo "exit: $?"
# stderr: {"error":"user_error","message":"invalid UUID: not-a-uuid"}
# exit: 1

# Exit 1: Invalid hex64
buzz messages delete --event "not-hex" 2>&1; echo "exit: $?"
# stderr: {"error":"user_error","message":"must be a 64-character hex string: not-hex"}
# exit: 1

# Exit 1: Invalid --type value (clap validates the enum — multi-line error)
buzz channels create --name x --type invalid --visibility open 2>&1; echo "exit: $?"
# stderr: {"error":"user_error","message":"error: invalid value 'invalid' for '--type <CHANNEL_TYPE>'\n  [possible values: stream, forum]\n..."}
# exit: 1

# Exit 1: Invalid --direction value
buzz messages vote --event "$(printf '0%.0s' {1..64})" \
  --direction sideways 2>&1; echo "exit: $?"
# exit: 1

# Exit 1: Empty body guard
buzz users set-profile 2>&1; echo "exit: $?"
# exit: 1 (at least one field required)

# Exit 3: No auth configured
env -u BUZZ_PRIVATE_KEY \
  cargo run -p buzz-cli -- channels list 2>&1; echo "exit: $?"
# stderr: {"error":"auth_error","message":"auth error: BUZZ_PRIVATE_KEY is required (use --private-key or set env var)"}
# exit: 3

# Not-found returns null, not an error (exit 0)
buzz channels get --channel "00000000-0000-0000-0000-000000000000"
# stdout: null
# exit: 0
```

---

## 8. Auth Testing

Test authentication.

```bash
# Private key (BUZZ_PRIVATE_KEY)
BUZZ_PRIVATE_KEY="nsec1..." buzz channels list | jq .
# Should succeed

# No auth → exit 3
env -u BUZZ_PRIVATE_KEY \
  cargo run -p buzz-cli -- channels list 2>&1; echo "exit: $?"
# stderr: {"error":"auth_error","message":"auth error: BUZZ_PRIVATE_KEY is required (use --private-key or set env var)"}
# exit: 3
```

---

## 9. Cleanup

```bash
# Delete test channels
buzz channels delete --channel "$CHANNEL_ID" | jq .
buzz channels delete --channel "$FORUM_ID" | jq .
```

---

## 10. Checklist

| # | Command | Tested | Notes |
|---|---------|:------:|-------|
| 1 | `messages send` | ☐ | Basic, reply, broadcast, mentions, stdin |
| 2 | `messages send-diff` | ☐ | Stdin, metadata, branch/PR |
| 3 | `messages edit` | ☐ | |
| 4 | `messages delete` | ☐ | |
| 5 | `messages get` | ☐ | With limit |
| 6 | `messages thread` | ☐ | |
| 7 | `messages search` | ☐ | With limit |
| 8 | `messages vote` | ☐ | Up and down |
| 9 | `channels list` | ☐ | With visibility, member |
| 10 | `channels get` | ☐ | |
| 11 | `channels create` | ☐ | Stream and forum |
| 12 | `channels update` | ☐ | |
| 13 | `channels topic` | ☐ | |
| 14 | `channels purpose` | ☐ | |
| 15 | `channels join` | ☐ | |
| 16 | `channels leave` | ☐ | |
| 17 | `channels archive` | ☐ | Needs admin:channels |
| 18 | `channels unarchive` | ☐ | Needs admin:channels |
| 19 | `channels delete` | ☐ | Needs admin:channels |
| 20 | `channels members` | ☐ | |
| 21 | `channels add-member` | ☐ | Needs admin:channels |
| 22 | `channels remove-member` | ☐ | Needs admin:channels |
| 23 | `canvas get` | ☐ | |
| 24 | `canvas set` | ☐ | Direct and stdin |
| 25 | `reactions add` | ☐ | |
| 26 | `reactions remove` | ☐ | |
| 27 | `reactions get` | ☐ | |
| 28 | `dms list` | ☐ | |
| 29 | `dms open` | ☐ | |
| 30 | `dms add-member` | ☐ | Needs messages:write |
| 31 | `users get` | ☐ | Self, single, batch |
| 32 | `users set-profile` | ☐ | |
| 33 | `users presence` | ☐ | |
| 34 | `users set-presence` | ☐ | online, away, offline |
| 35 | `workflows list` | ☐ | |
| 36 | `workflows create` | ☐ | |
| 37 | `workflows update` | ☐ | |
| 38 | `workflows delete` | ☐ | |
| 39 | `workflows trigger` | ☐ | |
| 40 | `workflows runs` | ☐ | |
| 41 | `workflows get` | ☐ | |
| 42 | `workflows approve` | ☐ | Validation only (needs approval gate); bare = approve, `--approved false` = deny |
| 43 | `feed get` | ☐ | |
| 44 | `social publish` | ☐ | |
| 45 | `social set-contacts` | ☐ | |
| 46 | `social event` | ☐ | |
| 47 | `social notes` | ☐ | |
| 48 | `social contacts` | ☐ | |
| 49 | `repos create` | ☐ | |
| 50 | `repos get` | ☐ | |
| 51 | `repos list` | ☐ | |
| 52 | `repos protect list` | ☐ | Empty/populated rules; unknown rules visible; malformed rule reported in validation_error |
| 53 | `repos protect set` | ☐ | Create and replace complete exact-ref rule; verify metadata is preserved |
| 54 | `repos protect remove` | ☐ | Remove exact ref; missing rule → NotFound |
| 55 | `upload file` | ☐ | |
| 56 | `pack validate` | ☐ | Local, no relay |
| 57 | `pack inspect` | ☐ | Local, no relay |
| 58 | `notes set` | ☐ | First publish, edit/carry, --clear-tags, ambiguity, empty-stdin guard |
| 59 | `notes get` | ☐ | By name, by naddr, --content-only, cross-author, ambiguous → exit 1 |
| 60 | `notes ls` | ☐ | Own, --author all, --tag, --limit |
| 61 | `notes rm` | ☐ | Delete→get 404, double-delete idempotent, missing slug → NotFound |
| 62 | `users set-status` | ☐ | Text+emoji, text only, emoji-only (`--text ""`), `--clear`, `--clear` + `--text` → exit 1 |

## Managed final-report delivery

`messages deliver` journals the first signed kind-9 event before publishing it:

```sh
DELIVERY_KEY=$(uuidgen)
JOURNAL_DIR="$PWD/.buzz-deliveries"
buzz messages deliver --delivery-key "$DELIVERY_KEY" --channel "$CHANNEL_ID" \
  --reply-to "$ROOT_EVENT_ID" --journal-dir "$JOURNAL_DIR" --content '@Callback final report'
buzz messages deliver --delivery-key "$DELIVERY_KEY" --channel "$CHANNEL_ID" \
  --reply-to "$ROOT_EVENT_ID" --journal-dir "$JOURNAL_DIR" --content 'changed summary'
```

Both commands return the first event ID. Read back with `messages get` or
`messages thread`: exactly one report bears `["buzz-delivery", DELIVERY_KEY]`,
and its content and original thread stay unchanged. `--mention <pubkey>` and
automatic member-name/nostr-URI mentions work as for `messages send` and remain
part of the first signed event. Use `messages edit` for an explicit correction;
use ordinary `messages send` for separate progress. Two fresh keys produce two
reports even with identical text. Reusing a key with another channel or parent
returns a conflict; a parent from another channel is also refused.

The default directory is `.buzz-deliveries` in the caller's working directory.
`--journal-dir` overrides `BUZZ_DELIVERY_JOURNAL_DIR`. Use an absolute persistent
directory across shell working-directory changes and restarts. ACP continuation
prompts supply their workspace directory explicitly. Namespace identity includes
the normalized relay URL, signer public key and delivery UUID; records bind the
original channel and parent. Membership is queried before each managed
submission and before returning a locally confirmed delivery. The relay also
enforces authorization on each HTTP request. Revoked access never redirects a
report to another channel or DM.

A lost/invalid acknowledgment leaves the signed event in the journal.
`managed_delivery_pending`, exit 2, `retryable:true` means retry the **same
managed command/key/route/journal**; it does not authorize a fresh `messages
send` or key. A valid acknowledgment must say `accepted:true` and name the saved
event ID. The real relay's `accepted:true, message:"duplicate:"` response is a
successful replay. Unjournaled `messages send` retains its existing
`delivery_unknown, retryable:false` ambiguity contract.

Late recovery requires an updated relay with atomic managed-report election.
For a report older than 600 seconds, the CLI journals a fresh signed copy that
preserves the original author, content and exact tags, then submits a signed
kind-20003 recovery command containing both events. An aged pending candidate
may be renewed again; the original evidence remains unchanged. The relay uses
one writer transaction to return a previously accepted report or insert one
current candidate and bind it to the completion key. The 960-second DB floor
remains active. Soft-deleted reports are acknowledged without resurrection.

The CLI validates and durably saves the chosen signed event before returning.
Recovered unaccepted reports can have a new event ID/date, with the same content,
mentions and original thread. A lost acknowledgment can return an earlier
candidate; callers must use the returned canonical ID. Journal v1 is migrated to
v2 under its existing lock. The record retains original evidence plus the latest candidate and canonical
timestamp/ID/signature; immutable content/tags are reconstructed and verified. Existing size/attempt limits
still apply. Old CLI versions reject v2 journals instead of publishing through
an incompatible path. Unknown relays cannot publish the nested candidate as a
kind-9 message, and ordinary wrapper acceptance is not sufficient confirmation.

Upgrade all relay writers and drain old in-flight writes before enabling this
recovery contract: mixed old writers do not participate in the canonical-key
lock. Each completion UUID must remain unique; the server retains its binding
until community deletion, independently of the journal's seven-day confirmed
retention. Key reuse with another body or route conflicts. Direct unmarked
progress and edit operations retain their separate behavior.

Each record permits eight managed submissions, each using the existing maximum
of three transport attempts (24 HTTP publication requests at most). Attempts
are saved before sending, so a crash can consume an attempt without publishing.
Exhaustion is explicit and preserves the signed evidence. Records are capped at
1024, 512 KiB each and 16 MiB serialized total. Capacity rejects new records;
unconfirmed/terminal records never silently expire or get evicted. Confirmed
records can expire after seven days; afterward this command no longer promises
idempotency for that old key. There is one lock file and one bounded snapshot,
with a fixed replacement file, and a 30-second bounded lock wait. New Unix
directories use 0700 and journal files use 0600. Snapshot replacement fsyncs both
file and directory before sending. Each invocation fsyncs every ancestor of the
journal directory, including existing links a concurrent creator may have
exposed before syncing. Paths deeper than 128 ancestors fail visibly.
Use a filesystem supporting advisory locks, atomic replacement and directory
fsync; unsupported filesystem/platform errors fail visibly before publication.
The durability workflow is exercised on macOS/Unix; Windows and network
filesystems have not been validated.

This covers cooperating standalone CLI and embedded CLI callers of
`messages deliver`. Other MCP/shell publishers do not share the journal. State
survives process restart only while this persistent volume survives. There is
no startup replay, no recovery of unjournaled provider output and no general
exactly-once guarantee. Recover a saved event by explicitly repeating its key
and route against the same journal. Keep pending/terminal records for inspection;
do not delete them to make a new send appear safe.

Automated local checks:

```sh
cargo test -p buzz-cli managed_ -- --test-threads=1
cargo build -p buzz-cli -p buzz-agent -p buzz-dev-mcp
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
  env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL \
  -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY \
  cargo test -p buzz-acp managed_runtime_ -- --ignored --test-threads=1
```

Those runtime checks use actual bundled agent/MCP/CLI processes, a deterministic
local model and a signature-checking **mock relay**, including publication
followed by ACP application error/retry, response-body loss, process kill after
relay acceptance and concurrent CLI processes. They are not evidence of a real
authenticated relay or human smoke test.

Human checks against an authorized local relay: run the commands above and
inspect event readback; repeat with a fresh key; send progress and edit the
managed report; run two same-key invocations concurrently; restart the CLI and
replay with changed text. Then use an authorized owner account to remove the
publisher's membership and repeat the original command: expect exit 3 and no
new publication or alternate route. Check the app shows one final report in the
original thread and preserves the intended callback mention. A crash/power-loss
check requires a disposable persisted journal and a relay/proxy boundary that
can stop the publisher after acceptance; preserve the journal and confirm
readback by event ID before recovery. Do not infer real power-loss durability
from process-kill tests alone.

## Session-status enrichment

An agent harness advertises its private status instance directory in
`BUZZ_SESSION_STATUS_DIR`. When it is set, `messages send` (kind 9, the
default) and the first preparation of `messages deliver` add two tags to the
new event before signing:

```text
["buzz-session", "<state>", "<reason>", "<percent or empty>"]
["buzz-session-info", "1", "<transition UUID>", "thread|conversation", "<read>", "<total>", "exact|lower_bound|unknown"]
```

The destination decides the snapshot. A reply matches the thread scope of its
canonical NIP-10 root (thread policy) or the channel's conversation scope
(channel policy and DMs); a top-level message matches only the conversation
scope. A message in another thread never borrows a thread's status. If both a
thread and a conversation snapshot are current, the scope is ambiguous and no
tags are added. Mixed execution scopes can exist after unresolved channel metadata
was conservatively admitted as a DM. Under thread policy, the harness withholds
or retires conversation display status unless metadata verifies a DM; this
includes ordinary turns of a retained session. Channel policy still permits
conversation status without channel metadata. A thread whose optional status
fails therefore cannot fall back to an uncertain conversation's telemetry.

Absence and failure never block or alter a valid publication: the message is
sent exactly as before, without status tags and without a diagnostic, when

- the variable is unset, empty, relative or not UTF-8, or its final component
  is not the instance UUID;
- the instance lock is missing or not held (the harness is gone);
- the snapshot is missing, not a regular file (for example a symlink), larger
  than 16 KiB, malformed, of another format version or otherwise invalid;
- the snapshot belongs to another relay or signing key (identity is the CLI's
  own relay URL and key, with `http`/`ws` and `https`/`wss` folded), another
  channel, thread root or instance;
- the snapshot's generation lock is missing or not held (a revoked generation
  whose file could not be removed), or the scope is ambiguous.

The lookup is read-only and bounded: it reads at most two snapshots, makes no
network calls, keeps no acknowledgment state and never creates, writes,
deletes or renames anything in the status directory. Locks are probed with
nonblocking attempts only; a lock the CLI can take is released at once and
treated as not held. A failed send therefore leaves the snapshot and its
transition untouched, and the next message still carries the explanation.

Only kind-9 messages are enriched. Forum posts and comments (`--kind 45001`,
`--kind 45003`), `messages send-diff`, edits, deletions and other commands keep
their existing tags. `messages deliver` journals the signed event with the
tags of its first preparation: a same-key retry and late timestamp recovery
replay those original tags even after the live status has changed, while a new
delivery key captures the current status.

Automated local checks (mock relay and a harness-shaped status directory with
real OS locks; no harness process):

```sh
cargo test -p buzz-cli session_status -- --test-threads=1
cargo test -p buzz-cli --test session_status_env
```

Human check against a local relay: run an agent under the harness, let it
reply in a thread, then read the reply back with
`buzz messages thread --channel "$CHANNEL_ID" --event "$REPLY_ID" | jq '.[].tags'`
and expect both tags. Stop the harness and send again with the same
environment: expect no status tags and a normal publication.
