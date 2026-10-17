import assert from "node:assert/strict";
import test from "node:test";
import { finalizeEvent, getPublicKey } from "nostr-tools/pure";

import { formatTimelineMessages } from "./formatTimelineMessages.ts";

// Session status is self-reported display metadata on an agent's own kind-9
// messages. These tests drive it through the production formatter: the
// explanation belongs to the earliest loaded message of one transition
// (signer, channel, scope, transition UUID) and every other message keeps
// only its reported percentage.

const AGENT_A =
  "1111111111111111111111111111111111111111111111111111111111111111";
const AGENT_B =
  "2222222222222222222222222222222222222222222222222222222222222222";
const HUMAN =
  "3333333333333333333333333333333333333333333333333333333333333333";
const RELAY_SECRET = new Uint8Array(32).fill(3);
const RELAY_PUBKEY = getPublicKey(RELAY_SECRET);
const CHANNEL = "36411e44-0e2d-4cfe-bd6e-567eb169db9f";
const OTHER_CHANNEL = "8d764100-fd8f-44cf-9c98-6d8fbd739b8c";
const T1 = "6f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";
const T2 = "0f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";
const ROOT_1 = "a".repeat(64);
const ROOT_2 = "b".repeat(64);

const PROFILES = {
  [AGENT_A]: { displayName: "Agent A", isAgent: true, ownerPubkey: HUMAN },
  [AGENT_B]: { displayName: "Agent B", isAgent: true, ownerPubkey: HUMAN },
  [HUMAN]: { displayName: "Human" },
};

let nextId = 1;
function hexId(n = nextId++) {
  return n.toString(16).padStart(64, "0");
}

function status(state, reason, percent = "") {
  return ["buzz-session", state, reason, percent];
}

function info(transition, scope, read = "", total = "", mode = "unknown") {
  return ["buzz-session-info", "1", transition, scope, read, total, mode];
}

function message({
  id = hexId(),
  pubkey = AGENT_A,
  createdAt = 1_700_000_000,
  channel = CHANNEL,
  root = null,
  kind = 9,
  tags = [],
}) {
  const threadTags = root
    ? [
        ["e", root, "", "root"],
        ["e", root, "", "reply"],
      ]
    : [];
  return {
    id,
    pubkey,
    kind,
    created_at: createdAt,
    content: `message ${id.slice(-4)}`,
    tags: [["h", channel], ...threadTags, ...tags],
    sig: "sig",
  };
}

function project(events, relaySelfPubkey = undefined) {
  const rows = formatTimelineMessages(
    events,
    null,
    HUMAN,
    null,
    PROFILES,
    undefined,
    undefined,
    undefined,
    relaySelfPubkey,
  );
  return new Map(rows.map((row) => [row.id, row.sessionStatus]));
}

const NEW_EXPLANATION = { title: "New session" };

test("first message of a transition explains it; later ones keep only the percentage", () => {
  const first = message({
    createdAt: 10,
    tags: [
      status("new", "first_in_thread"),
      info(T1, "conversation", "12", "37", "exact"),
    ],
  });
  const second = message({
    createdAt: 20,
    tags: [status("new", "first_in_thread", "64"), info(T1, "conversation")],
  });
  const third = message({
    createdAt: 30,
    tags: [status("new", "first_in_thread"), info(T1, "conversation")],
  });
  const out = project([first, second, third]);
  assert.deepEqual(out.get(first.id), {
    explanation: {
      title: "New session",
      history: ["read 12 of 37 messages"],
    },
  });
  assert.deepEqual(out.get(second.id), { percent: 64 });
  // Unknown occupancy on a non-owner: no row at all, never 0%.
  assert.equal(out.get(third.id), undefined);
});

test("the owner shows its own percentage beside the explanation", () => {
  const first = message({
    tags: [status("resumed", "idle_sleep", "81"), info(T1, "conversation")],
  });
  assert.deepEqual(project([first]).get(first.id), {
    explanation: { title: "Resumed after sleep" },
    percent: 81,
  });
});

test("two agents with the same transition UUID each keep their own explanation", () => {
  const a1 = message({
    pubkey: AGENT_A,
    createdAt: 10,
    tags: [status("new", "first_in_thread"), info(T1, "conversation")],
  });
  const b1 = message({
    pubkey: AGENT_B,
    createdAt: 11,
    tags: [status("resumed", "harness_restart", "5"), info(T1, "conversation")],
  });
  const a2 = message({
    pubkey: AGENT_A,
    createdAt: 12,
    tags: [status("new", "first_in_thread", "7"), info(T1, "conversation")],
  });
  const b2 = message({
    pubkey: AGENT_B,
    createdAt: 13,
    tags: [status("resumed", "harness_restart", "9"), info(T1, "conversation")],
  });
  const out = project([b2, a2, b1, a1]);
  assert.deepEqual(out.get(a1.id), { explanation: NEW_EXPLANATION });
  assert.deepEqual(out.get(b1.id), {
    explanation: { title: "Resumed after restart" },
    percent: 5,
  });
  assert.deepEqual(out.get(a2.id), { percent: 7 });
  assert.deepEqual(out.get(b2.id), { percent: 9 });
});

test("thread scopes, conversation scope and channels never share an explanation", () => {
  const tags = (scope) => [status("new", "first_in_thread"), info(T1, scope)];
  const thread1 = message({
    createdAt: 10,
    root: ROOT_1,
    tags: tags("thread"),
  });
  const thread2 = message({
    createdAt: 11,
    root: ROOT_2,
    tags: tags("thread"),
  });
  const conversation = message({
    createdAt: 12,
    tags: tags("conversation"),
  });
  const otherChannel = message({
    createdAt: 13,
    channel: OTHER_CHANNEL,
    tags: tags("conversation"),
  });
  const out = project([thread1, thread2, conversation, otherChannel]);
  for (const event of [thread1, thread2, conversation, otherChannel]) {
    assert.deepEqual(out.get(event.id), { explanation: NEW_EXPLANATION });
  }
});

test("conversation scope groups across top-level messages and thread replies", () => {
  const top = message({
    createdAt: 10,
    tags: [status("new", "first_in_thread"), info(T1, "conversation")],
  });
  const inThread = message({
    createdAt: 11,
    root: ROOT_1,
    tags: [status("new", "first_in_thread", "3"), info(T1, "conversation")],
  });
  const out = project([top, inThread]);
  assert.deepEqual(out.get(top.id), { explanation: NEW_EXPLANATION });
  assert.deepEqual(out.get(inThread.id), { percent: 3 });
});

test("interleaved human replies do not reset a thread transition", () => {
  const agent1 = message({
    createdAt: 10,
    root: ROOT_1,
    tags: [status("resumed", "worker_crash", "30"), info(T2, "thread")],
  });
  const human1 = message({ pubkey: HUMAN, createdAt: 11, root: ROOT_1 });
  const agent2 = message({
    createdAt: 12,
    root: ROOT_1,
    tags: [status("resumed", "worker_crash", "31"), info(T2, "thread")],
  });
  const human2 = message({ pubkey: HUMAN, createdAt: 13, root: ROOT_1 });
  const agent3 = message({
    createdAt: 14,
    root: ROOT_1,
    tags: [status("resumed", "worker_crash", "32"), info(T2, "thread")],
  });
  const out = project([agent1, human1, agent2, human2, agent3]);
  assert.deepEqual(out.get(agent1.id), {
    explanation: { title: "Resumed after worker failure" },
    percent: 30,
  });
  assert.equal(out.get(human1.id), undefined);
  assert.deepEqual(out.get(agent2.id), { percent: 31 });
  assert.deepEqual(out.get(agent3.id), { percent: 32 });
});

test("same-second messages order by event id, independent of input order", () => {
  const low = message({
    id: hexId(0x100),
    createdAt: 50,
    tags: [status("new", "first_in_thread", "1"), info(T1, "conversation")],
  });
  const high = message({
    id: hexId(0x200),
    createdAt: 50,
    tags: [status("new", "first_in_thread", "2"), info(T1, "conversation")],
  });
  for (const order of [
    [low, high],
    [high, low],
  ]) {
    const out = project(order);
    assert.deepEqual(out.get(low.id), {
      explanation: NEW_EXPLANATION,
      percent: 1,
    });
    assert.deepEqual(out.get(high.id), { percent: 2 });
  }
});

test("deleting the owner moves the explanation to the next loaded message", () => {
  const first = message({
    createdAt: 10,
    tags: [status("new", "first_in_thread"), info(T1, "conversation")],
  });
  const second = message({
    createdAt: 20,
    tags: [status("new", "first_in_thread", "40"), info(T1, "conversation")],
  });
  const deletion = {
    id: hexId(),
    pubkey: AGENT_A,
    kind: 5,
    created_at: 30,
    content: "",
    tags: [
      ["h", CHANNEL],
      ["e", first.id],
    ],
    sig: "sig",
  };
  const out = project([first, second, deletion]);
  assert.equal(out.has(first.id), false);
  assert.deepEqual(out.get(second.id), {
    explanation: NEW_EXPLANATION,
    percent: 40,
  });
});

test("an earlier page entering the window takes over the explanation", () => {
  const older = message({
    createdAt: 10,
    tags: [status("new", "first_in_thread"), info(T1, "conversation")],
  });
  const newer = message({
    createdAt: 20,
    tags: [status("new", "first_in_thread", "55"), info(T1, "conversation")],
  });
  const before = project([newer]);
  assert.deepEqual(before.get(newer.id), {
    explanation: NEW_EXPLANATION,
    percent: 55,
  });
  const after = project([older, newer]);
  assert.deepEqual(after.get(older.id), { explanation: NEW_EXPLANATION });
  assert.deepEqual(after.get(newer.id), { percent: 55 });
});

test("an edit cannot add or change status; the original signed tags win", () => {
  const original = message({
    createdAt: 10,
    tags: [status("new", "first_in_thread", "20"), info(T1, "conversation")],
  });
  const plain = message({ createdAt: 11 });
  const editOf = (target, createdAt) => ({
    id: hexId(),
    pubkey: AGENT_A,
    kind: 40003,
    created_at: createdAt,
    content: "edited",
    tags: [
      ["h", CHANNEL],
      ["e", target.id],
      status("resumed", "harness_restart", "99"),
      info(T2, "conversation", "1", "1", "exact"),
    ],
    sig: "sig",
  });
  const rows = formatTimelineMessages(
    [original, plain, editOf(original, 20), editOf(plain, 21)],
    null,
    HUMAN,
    null,
    PROFILES,
  );
  const byId = new Map(rows.map((row) => [row.id, row]));
  assert.equal(byId.get(original.id).body, "edited");
  assert.deepEqual(byId.get(original.id).sessionStatus, {
    explanation: NEW_EXPLANATION,
    percent: 20,
  });
  assert.equal(byId.get(plain.id).edited, true);
  assert.equal(byId.get(plain.id).sessionStatus, undefined);
});

test("a human copying status tags gets no status row", () => {
  const copied = message({
    pubkey: HUMAN,
    tags: [status("new", "first_in_thread", "50"), info(T1, "conversation")],
  });
  assert.equal(project([copied]).get(copied.id), undefined);
});

test("a relay-signed event attributed to an agent cannot claim its session state", () => {
  const event = finalizeEvent(
    {
      kind: 9,
      created_at: 1_700_000_000,
      content: "relayed",
      tags: [
        ["h", CHANNEL],
        ["actor", AGENT_A],
        status("new", "first_in_thread", "50"),
        info(T1, "conversation"),
      ],
    },
    RELAY_SECRET,
  );
  const out = formatTimelineMessages(
    [event],
    null,
    HUMAN,
    null,
    PROFILES,
    undefined,
    undefined,
    undefined,
    RELAY_PUBKEY,
  );
  assert.equal(out[0].pubkey, AGENT_A);
  assert.equal(out[0].isAgent, true);
  assert.equal(out[0].sessionStatus, undefined);
});

test("a locally pending event cannot own an explanation", () => {
  const pending = {
    ...message({
      tags: [status("new", "first_in_thread", "50"), info(T1, "conversation")],
    }),
    pending: true,
  };
  assert.equal(project([pending]).get(pending.id), undefined);
});

test("non-message kinds from an agent carry no status", () => {
  const system = message({
    kind: 40099,
    tags: [status("new", "first_in_thread", "50"), info(T1, "conversation")],
  });
  assert.equal(project([system]).get(system.id), undefined);
});

test("missing or invalid supporting metadata yields no explanation or counts", () => {
  const missing = message({
    createdAt: 10,
    tags: [status("new", "first_in_thread", "12")],
  });
  const invalid = message({
    createdAt: 11,
    tags: [
      status("new", "first_in_thread", "13"),
      ["buzz-session-info", "2", T1, "conversation", "3", "4", "exact"],
    ],
  });
  const unknownNoPercent = message({
    createdAt: 12,
    tags: [status("new", "first_in_thread")],
  });
  const out = project([missing, invalid, unknownNoPercent]);
  assert.deepEqual(out.get(missing.id), { percent: 12 });
  assert.deepEqual(out.get(invalid.id), { percent: 13 });
  assert.equal(out.get(unknownNoPercent.id), undefined);
});

test("thread scope without a thread root has no grouping key", () => {
  const orphan = message({
    tags: [status("new", "first_in_thread", "8"), info(T1, "thread")],
  });
  assert.deepEqual(project([orphan]).get(orphan.id), { percent: 8 });
});

test("invalid primary tag yields no status even with valid supporting metadata", () => {
  const bad = message({
    tags: [
      status("new", "resume_failed:/home/user/secret", "12"),
      info(T1, "conversation", "1", "2", "exact"),
    ],
  });
  assert.equal(project([bad]).get(bad.id), undefined);
});

test("resume failure, unknown context and incomplete history read truthfully", () => {
  const event = message({
    root: ROOT_1,
    tags: [
      status("new", "resume_failed:not_found"),
      info(T1, "thread", "12", "", "lower_bound"),
    ],
  });
  assert.deepEqual(project([event]).get(event.id), {
    explanation: {
      title: "New session",
      detail: "resume failed: previous session not found",
      history: ["read 12 messages", "Earlier history may be omitted"],
    },
  });
});

test("rows without status tags keep the old shape", () => {
  const plain = message({ pubkey: HUMAN });
  const [row] = formatTimelineMessages([plain], null, HUMAN, null, PROFILES);
  assert.equal("sessionStatus" in row, false);
});
