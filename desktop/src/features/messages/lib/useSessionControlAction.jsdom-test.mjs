import assert from "node:assert/strict";
import { test, afterEach } from "node:test";
import { act, renderHook, cleanup } from "@testing-library/react";
import { useSessionControlAction } from "./useSessionControlAction.ts";
afterEach(cleanup);
const scope = {
  targetPubkey: "a".repeat(64),
  signerPubkey: "b".repeat(64),
  channelId: "channel",
  rootEventId: "c".repeat(64),
  relayUrl: "wss://example.test",
};
const command = { type: "restart", depth: "none", content: "!restart none" };
const evidence = {
  targetPubkey: scope.targetPubkey,
  agent: {
    pubkey: scope.targetPubkey,
    ownerPubkey: scope.signerPubkey,
    respondTo: "owner-only",
    respondToAllowlist: [],
    channelIds: ["channel"],
  },
  channel: {
    id: "channel",
    channelType: "stream",
    isMember: true,
    archivedAt: null,
  },
};
function setup() {
  let resolve, reject;
  const promise = new Promise((r, fail) => {
    resolve = r;
    reject = fail;
  });
  const sends = [];
  const dependencies = {
    fetchEvidence: () => promise,
    send: async (...args) => {
      sends.push(args);
    },
  };
  const hook = renderHook(
    (props) => useSessionControlAction(props, dependencies),
    { initialProps: scope },
  );
  return { hook, resolve, reject, sends, dependencies };
}
test("captures exact target and canonical thread routing, publishes once", async () => {
  const { hook, resolve, sends } = setup();
  let pending;
  act(() => {
    pending = hook.result.current.run(command);
  });
  assert.equal(await hook.result.current.run(command), false);
  await act(async () => {
    resolve(evidence);
    await pending;
  });
  assert.equal(sends.length, 1);
  assert.deepEqual(sends[0], [
    "channel",
    "!restart none",
    scope.rootEventId,
    undefined,
    [scope.targetPubkey],
    9,
    undefined,
    undefined,
    undefined,
    undefined,
    scope.relayUrl,
    scope.signerPubkey,
    scope.rootEventId,
  ]);
  assert.equal(hook.result.current.pending, false);
});
for (const boundary of [
  "revoke",
  "query failure",
  "community",
  "signer",
  "target",
  "root",
  "channel",
  "unmount",
  "publication failure",
]) {
  test(`no unintended publication after ${boundary}`, async () => {
    const { hook, resolve, reject, sends, dependencies } = setup();
    if (boundary === "publication failure")
      dependencies.send = async () => {
        throw new Error("offline");
      };
    let pending;
    act(() => {
      pending = hook.result.current.run(command);
    });
    if (boundary === "unmount") hook.unmount();
    if (["community", "signer", "target", "root", "channel"].includes(boundary))
      hook.rerender({
        ...scope,
        [{
          community: "relayUrl",
          signer: "signerPubkey",
          target: "targetPubkey",
          root: "rootEventId",
          channel: "channelId",
        }[boundary]]: "changed",
      });
    await act(async () => {
      if (boundary === "query failure")
        reject(new Error("directory unavailable"));
      else
        resolve(
          boundary === "revoke" ? { ...evidence, agent: null } : evidence,
        );
      assert.equal(await pending, false);
    });
    assert.equal(sends.length, 0);
    if (["revoke", "query failure", "publication failure"].includes(boundary))
      assert.match(hook.result.current.error, /Could not send/);
  });
}

test("switching away and back cannot revive an old action", async () => {
  const { hook, resolve, sends } = setup();
  let pending;
  act(() => {
    pending = hook.result.current.run(command);
  });
  hook.rerender({ ...scope, rootEventId: "other" });
  hook.rerender(scope);
  await act(async () => {
    resolve(evidence);
    assert.equal(await pending, false);
  });
  assert.equal(sends.length, 0);
});

test("manual actor joins the actual channel surface batch without a per-row fetch", async () => {
  const { useMessageEventProfilePubkeys } = await import(
    "../../channels/useMessageEventProfilePubkeys.ts"
  );
  const actor = "d".repeat(64),
    transition = "6f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";
  const event = {
    id: "e".repeat(64),
    pubkey: scope.targetPubkey,
    kind: 9,
    created_at: 1,
    content: "Done",
    sig: "sig",
    tags: [
      ["buzz-session", "compacted", "user_compact", ""],
      ["buzz-session-info", "1", transition, "thread", "", "", "unknown"],
      ["buzz-session-action", "1", transition, "f".repeat(64), actor],
    ],
  };
  const hook = renderHook(() =>
    useMessageEventProfilePubkeys([event], [], null),
  );
  assert.ok(hook.result.current.includes(actor));
});
