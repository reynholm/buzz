import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs";
import { parseSessionCommand } from "./sessionControl.ts";
import { formatTimelineMessages } from "./formatTimelineMessages.ts";
import { hasSameMessageAuthor } from "./messageGrouping.ts";
const A = "a".repeat(64),
  B = "b".repeat(64),
  H = "c".repeat(64),
  ID = "d".repeat(64);
const profiles = {
  [A]: { displayName: "Agent A", isAgent: true },
  [B]: { displayName: "Agent B", isAgent: true },
  [H]: { displayName: "Human" },
};
const request = (extra = {}) => ({
  id: ID,
  pubkey: H,
  kind: 9,
  content: "!restart",
  created_at: Math.floor(Date.now() / 1000),
  tags: [
    ["h", "channel"],
    ["p", A],
  ],
  sig: "sig",
  ...extra,
});
const reaction = (outcome = "ok", code = "", extra = {}) => ({
  id: "e".repeat(64),
  pubkey: A,
  kind: 7,
  content: outcome === "ok" ? "✅" : outcome === "pending" ? "⏳" : "❌",
  created_at: 3,
  tags: [
    ["h", "channel"],
    ["e", ID],
    ["buzz-session-result", "1", ID, outcome, code],
  ],
  sig: "sig",
  ...extra,
});
const rows = (events) =>
  formatTimelineMessages(events, null, H, null, profiles);
const control = (events) =>
  rows(events).find((row) => row.id === ID)?.sessionControl;
test("fresh requests wait for the agent without claiming execution or confirmation", () => {
  const command = request();
  assert.deepEqual(control([command]).targets[0], {
    pubkey: A,
    name: "Agent A",
    state: "awaiting",
    detail: "Waiting for agent…",
    expiresAt: command.created_at + 901,
  });
});
test("shared exact grammar matches the actual core parser whitespace corpus", () => {
  const fixture = JSON.parse(
    fs.readFileSync(
      new URL(
        "../../../../../crates/buzz-core/tests/fixtures/session_controls.json",
        import.meta.url,
      ),
    ),
  );
  for (const entry of fixture.commands)
    assert.equal(
      parseSessionCommand(entry.content)?.content ?? null,
      entry.expected,
      JSON.stringify(entry.content),
    );
});
test("formatter reads original signed kind9 body, keeps identity, and requires known exact targets", () => {
  const original = request();
  const edit = {
    ...request(),
    id: "f".repeat(64),
    kind: 40003,
    content: "ordinary edit",
    created_at: original.created_at + 1,
    tags: [
      ["h", "channel"],
      ["e", ID],
    ],
  };
  assert.ok(control([original, edit]));
  assert.equal(
    control([
      request({ content: "ordinary" }),
      { ...edit, content: "!compact" },
    ]),
    undefined,
  );
  for (const value of [
    request({ kind: 40002 }),
    request({ tags: [["p", H]] }),
    request({ content: "!restart recent" }),
    request({ content: "!compact @Agent A" }),
  ])
    assert.equal(control([value]), undefined);
  const row = rows([original])[0];
  assert.equal(row.id, ID);
  assert.equal(row.kind, 9);
  assert.equal(row.depth, 0);
  assert.equal(row.sessionControl.action, "Start over with recent history");
  assert.equal(hasSameMessageAuthor(row, { pubkey: H }), false);
  assert.equal(hasSameMessageAuthor({ pubkey: H }, row), false);
});
test("results require exact target raw signer, exact e reference and bounded tag; conflicts are unconfirmed", () => {
  assert.equal(control([request(), reaction()]).targets[0].state, "success");
  assert.equal(
    control([request(), reaction("pending")]).targets[0].detail,
    "Waiting for current work to stop",
  );
  assert.equal(
    control([request({ content: "!compact" }), reaction("pending")]).targets[0]
      .detail,
    "Waiting for current work to finish · Compaction will start automatically",
  );
  assert.equal(
    control([request(), reaction("pending"), reaction()]).targets[0].state,
    "success",
  );
  assert.equal(
    control([
      request(),
      reaction(),
      reaction("error", "busy", { id: "1".repeat(64) }),
    ]).targets[0].state,
    "unconfirmed",
  );
  for (const bad of [
    reaction("ok", "", { pubkey: H }),
    reaction("ok", "", { pubkey: B }),
    reaction("ok", "", {
      tags: [
        ["e", "0".repeat(64)],
        ["buzz-session-result", "1", ID, "ok", ""],
      ],
    }),
    reaction("ok", "", { tags: [["e", ID]] }),
    reaction("ok", "", {
      tags: [
        ["e", ID],
        ["buzz-session-result", "1", ID, "ok", "raw/path"],
      ],
    }),
    reaction("ok", "", { pending: true }),
  ])
    assert.notEqual(control([request(), bad]).targets[0].state, "success");
  const multi = control([
    request({
      tags: [
        ["p", A],
        ["p", B],
      ],
    }),
    reaction(),
  ]);
  assert.equal(multi.targets[0].state, "success");
  assert.notEqual(multi.targets[1].state, "success");
  assert.equal(
    control([request({ created_at: 1 })]).targets[0].state,
    "expired",
  );
  assert.equal(
    control([request({ pending: true }), reaction()]).targets[0].state,
    "publishing",
  );
});
const T = "6f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";
test("compact wait expires without a terminal and fresh wait reports cover late admission", () => {
  const now = Math.floor(Date.now() / 1000);
  const command = request({ content: "!compact", created_at: now - 2000 });
  const oldWait = reaction("pending", "", { created_at: now - 1100 });
  assert.equal(control([command, oldWait]).targets[0].state, "expired");
  const freshWait = reaction("pending", "", { created_at: now });
  assert.equal(control([command, freshWait]).targets[0].state, "pending");
  assert.equal(control([command, freshWait]).targets[0].expiresAt, now + 1050);
  assert.equal(
    control([command, oldWait, reaction()]).targets[0].state,
    "success",
  );
});
test("manual status uses matching bounded actor metadata and neutral fallback without it", () => {
  const tags = [
    ["h", "channel"],
    ["buzz-session", "compacted", "user_compact", "18"],
    ["buzz-session-info", "1", T, "conversation", "", "", "unknown"],
    ["buzz-session-action", "1", T, ID, H],
  ];
  const event = request({ pubkey: A, content: "Done", tags });
  assert.equal(
    rows([event])[0].sessionStatus.explanation.title,
    "Context compacted by Human",
  );
  assert.equal(
    rows([{ ...event, tags: tags.slice(0, -1) }])[0].sessionStatus.explanation
      .title,
    "Context compacted",
  );
  assert.equal(
    rows([
      { ...event, tags: [...tags, ["buzz-session-action", "1", T, ID, B]] },
    ])[0].sessionStatus.explanation.title,
    "Context compacted",
  );
  assert.equal(
    rows([
      {
        ...event,
        tags: tags.map((tag) =>
          tag[0] === "buzz-session-action"
            ? [...tag.slice(0, 2), "wrong", ID, H]
            : tag,
        ),
      },
    ])[0].sessionStatus.explanation.title,
    "Context compacted",
  );
});

test("known bot membership identifies targets even before optional profile metadata arrives", () => {
  const [row] = formatTimelineMessages([request()], null, H, null, {}, [
    { pubkey: A, role: "bot" },
  ]);
  assert.ok(row.sessionControl);
});

test("delegated relay reaction pills cannot confirm a target-signed session result", async () => {
  const { finalizeEvent, getPublicKey } = await import("nostr-tools/pure");
  const secret = new Uint8Array(32).fill(3),
    relay = getPublicKey(secret);
  const delegated = finalizeEvent(
    {
      kind: 7,
      created_at: 3,
      content: "✅",
      tags: [
        ["h", "channel"],
        ["actor", A],
        ["e", ID],
        ["buzz-session-result", "1", ID, "ok", ""],
      ],
    },
    secret,
  );
  const [row] = formatTimelineMessages(
    [request(), delegated],
    null,
    H,
    null,
    profiles,
    undefined,
    undefined,
    undefined,
    relay,
  );
  assert.equal(row.reactions[0].users[0].pubkey, A);
  assert.equal(row.sessionControl.targets[0].state, "awaiting");
});
test("deleting an outcome removes confirmation and malformed conflicting enrichment stays unconfirmed", () => {
  const result = reaction();
  const deleted = request({
    id: "1".repeat(64),
    kind: 5,
    content: "",
    tags: [["e", result.id]],
  });
  assert.equal(
    control([request(), result, deleted]).targets[0].state,
    "awaiting",
  );
  const malformed = reaction("ok", "", {
    id: "2".repeat(64),
    tags: [
      ["e", ID],
      ["buzz-session-result", "1", ID, "ok", "raw/path"],
    ],
  });
  assert.equal(
    control([request(), result, malformed]).targets[0].state,
    "unconfirmed",
  );
});

test("ordinary target reactions have no authority over a separate structured result", () => {
  const ordinary = reaction("ok", "", {
    id: "3".repeat(64),
    content: "👍",
    tags: [
      ["h", "channel"],
      ["e", ID],
    ],
  });
  assert.equal(
    control([request(), reaction(), ordinary]).targets[0].state,
    "success",
  );
});
