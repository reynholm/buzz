import assert from "node:assert/strict";
import { test } from "node:test";
import {
  hasPersistedThread,
  projectThreadContextAgents,
} from "./threadContextAgents.ts";
import { loadThreadReplies } from "../useThreadReplies.ts";
import { QueryClient } from "@tanstack/react-query";
const root = "1".repeat(64),
  nested = "2".repeat(64),
  human = "3".repeat(64),
  a = "a".repeat(64),
  b = "b".repeat(64),
  c = "c".repeat(64);
const channel = {
  id: "channel",
  channelType: "stream",
  isMember: true,
  archivedAt: null,
};
const bot = (pubkey, extra = {}) => ({
  pubkey,
  name: "Scout",
  ownerPubkey: human,
  respondTo: "owner-only",
  respondToAllowlist: [],
  channelIds: [channel.id],
  ...extra,
});
const event = (id, pubkey, tags = []) => ({
  id,
  pubkey,
  kind: 9,
  tags: [["h", channel.id], ...tags],
  created_at: 1,
  content: "",
  sig: "",
});
const rootEvent = event(root, human, [["p", a]]);
function project(events, directory = [bot(a), bot(b), bot(c)], extra = {}) {
  return projectThreadContextAgents({
    rootId: root,
    channel,
    signerPubkey: human,
    directory,
    events,
    ...extra,
  });
}

test("only persisted nonpending heads establish a scope, independent of a draft", () => {
  for (const head of [
    null,
    {},
    { id: root },
    { id: root, signerPubkey: human, pending: true },
    { id: "loading", signerPubkey: human },
  ])
    assert.equal(hasPersistedThread(head), false);
  assert.equal(hasPersistedThread({ id: root, signerPubkey: human }), true);
});
test("root-only addressed agents and raw nested signers deduplicate without sibling or display attribution leakage", () => {
  assert.deepEqual(
    project([rootEvent]).map((agent) => agent.pubkey),
    [a],
  );
  const events = [
    rootEvent,
    event(nested, b, [
      ["e", root, "", "reply"],
      ["p", a],
      ["p", a],
    ]),
    event("4".repeat(64), c),
    event("5".repeat(64), human, [
      ["e", root, "", "reply"],
      ["author", c],
    ]),
    {
      ...event("6".repeat(64), c, [["e", root, "", "reply"]]),
      tags: [
        ["h", "other"],
        ["e", root, "", "reply"],
      ],
    },
  ];
  const agents = project(events);
  assert.deepEqual(
    agents.map((agent) => agent.pubkey),
    [a, b],
  );
  assert.notEqual(agents[0].label, agents[1].label);
  assert.equal(
    project(
      [rootEvent],
      [bot(a, { respondTo: "allowlist", respondToAllowlist: [] })],
      { signerPubkey: c },
    ).length,
    0,
  );
  assert.deepEqual(project(events, [bot(a)], { signerPubkey: c }), []);
});
test("permission changes and unavailable evidence never grant control; telemetry and mention pins are irrelevant", () => {
  for (const extra of [
    { channel: { ...channel, isMember: false } },
    { channel: { ...channel, archivedAt: 1 } },
    { directory: [] },
    { signerPubkey: "" },
  ])
    assert.deepEqual(project([rootEvent], undefined, extra), []);
  const stranger = "d".repeat(64);
  assert.equal(
    project([rootEvent], [bot(a, { respondTo: "anyone" })], {
      signerPubkey: stranger,
    }).length,
    1,
  );
  assert.equal(
    project([rootEvent], [bot(a, { respondTo: "anyone" })], {
      signerPubkey: stranger,
      channel: { ...channel, channelType: "dm" },
    }).length,
    0,
  );
  assert.equal(project([rootEvent]).length, 1);
});
test("bounded production thread loader includes later-page collapsed participants and fails rather than returning partial history", async () => {
  const client = new QueryClient();
  let calls = 0;
  const fetcher = async () =>
    ++calls === 1
      ? {
          events: [event(nested, b, [["e", root, "", "reply"]])],
          nextCursor: { id: nested, createdAt: 1 },
        }
      : {
          events: [event("7".repeat(64), c, [["e", root, "", "reply"]])],
          nextCursor: null,
        };
  const replies = await loadThreadReplies(
    client,
    channel.id,
    root,
    null,
    undefined,
    fetcher,
  );
  assert.equal(calls, 2);
  assert.deepEqual(
    project([rootEvent, ...replies]).map((agent) => agent.pubkey),
    [a, b, c],
  );
  calls = 0;
  await assert.rejects(
    loadThreadReplies(
      client,
      channel.id,
      root,
      null,
      undefined,
      async (...args) => {
        if (calls === 1) throw new Error("page unavailable");
        return fetcher(...args);
      },
    ),
    /page unavailable/,
  );
  client.clear();
});

test("a quoted root in a sibling never contributes an agent; forum roots remain eligible", () => {
  assert.deepEqual(
    project([
      rootEvent,
      event("8".repeat(64), c, [
        ["e", root, "", "mention"],
        ["e", "9".repeat(64), "", "reply"],
      ]),
    ]).map((agent) => agent.pubkey),
    [a],
  );
  assert.deepEqual(
    project([{ ...rootEvent, kind: 40002 }]).map((agent) => agent.pubkey),
    [a],
  );
});
