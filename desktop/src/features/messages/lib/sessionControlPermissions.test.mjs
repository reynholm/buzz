import assert from "node:assert/strict";
import test from "node:test";
import { canManageSession } from "./sessionControlPermissions.ts";
const target = "a".repeat(64),
  owner = "b".repeat(64),
  viewer = "c".repeat(64);
const evidence = (policy = "owner-only", type = "stream") => ({
  targetPubkey: target,
  agent: {
    pubkey: target,
    ownerPubkey: owner,
    respondTo: policy,
    respondToAllowlist: [viewer],
    channelIds: ["channel"],
  },
  channel: {
    id: "channel",
    channelType: type,
    isMember: true,
    archivedAt: null,
  },
});
test("verified scope and exact policy evidence admit owner, sibling, allowlist and anyone", () => {
  for (const policy of ["owner-only", "allowlist", "anyone"]) {
    for (const type of ["stream", "forum", "dm"]) {
      const e = evidence(policy, type);
      assert.equal(canManageSession(e, owner, "channel"), true);
      assert.equal(
        canManageSession(
          { ...e, requester: { pubkey: viewer, ownerPubkey: owner } },
          viewer,
          "channel",
        ),
        true,
      );
      assert.equal(
        canManageSession(e, viewer, "channel"),
        type !== "dm" && policy !== "owner-only",
      );
    }
  }
});
test("unknown, stale local inventory, missing membership and wrong coordinates never grant authority", () => {
  const e = evidence("anyone");
  for (const patch of [
    { agent: null },
    { agent: { ...e.agent, respondTo: null } },
    { agent: { ...e.agent, respondTo: "nobody" } },
    { agent: { ...e.agent, pubkey: viewer } },
    { agent: { ...e.agent, channelIds: [] } },
    { channel: null },
    { channel: { ...e.channel, channelType: "unknown" } },
    { channel: { ...e.channel, isMember: false } },
    { channel: { ...e.channel, id: "other" } },
    { agent: null, managedAgent: { pubkey: target, respondTo: "anyone" } },
  ])
    assert.equal(canManageSession({ ...e, ...patch }, owner, "channel"), false);
  assert.equal(
    canManageSession(
      { ...evidence(), requester: { pubkey: target, ownerPubkey: owner } },
      viewer,
      "channel",
    ),
    false,
  );
});
test("remote verified cross-owner targets remain allowed and telemetry is optional", () => {
  assert.equal(canManageSession(evidence("anyone"), viewer, "channel"), true);
  assert.equal(
    canManageSession(evidence("allowlist"), viewer, "channel"),
    true,
  );
});
