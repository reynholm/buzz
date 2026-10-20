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
test("publishes the audience callback only after a successful current-scope send", async () => {
  const { hook, resolve, dependencies } = setup();
  let sent, finish;
  dependencies.send = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  let calls = 0;
  act(() => {
    sent = hook.result.current.run(command, () => {
      calls += 1;
    });
  });
  assert.equal(hook.result.current.pending, true);
  assert.equal(calls, 0);
  await act(async () => {
    resolve(evidence);
  });
  assert.equal(calls, 0);
  await act(async () => {
    finish();
    assert.equal(await sent, true);
  });
  assert.equal(calls, 1);
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
    let commits = 0;
    act(() => {
      pending = hook.result.current.run(command, () => {
        commits += 1;
      });
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
    assert.equal(commits, 0);
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
test("a scope change during native publication retires its audience callback", async () => {
  const { hook, resolve, dependencies } = setup();
  let finish, pending;
  let calls = 0;
  dependencies.send = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  act(() => {
    pending = hook.result.current.run(command, () => {
      calls += 1;
    });
  });
  await act(async () => {
    resolve(evidence);
  });
  hook.rerender({ ...scope, rootEventId: "other" });
  hook.rerender(scope);
  await act(async () => {
    finish();
    assert.equal(await pending, false);
  });
  assert.equal(calls, 0);
});
for (const roundTrip of [false, true])
  test(`retired flights cannot block or release a newer flight (round trip ${roundTrip})`, async () => {
    const finishes = [];
    const dependencies = {
      fetchEvidence: async (target) => ({
        ...evidence,
        targetPubkey: target.targetPubkey,
        agent: { ...evidence.agent, pubkey: target.targetPubkey },
      }),
      send: () => new Promise((resolve) => finishes.push(resolve)),
    };
    const hook = renderHook(
      (value) => useSessionControlAction(value, dependencies),
      { initialProps: scope },
    );
    let first, second;
    await act(async () => {
      first = hook.result.current.run(command);
    });
    hook.rerender({ ...scope, targetPubkey: "e".repeat(64) });
    if (roundTrip) hook.rerender(scope);
    assert.equal(hook.result.current.pending, false);
    await act(async () => {
      second = hook.result.current.run(command);
    });
    assert.equal(finishes.length, 2);
    await act(async () => {
      finishes[0]();
      assert.equal(await first, false);
    });
    assert.equal(hook.result.current.pending, true);
    assert.equal(await hook.result.current.run(command), false);
    await act(async () => {
      finishes[1]();
      assert.equal(await second, true);
    });
    assert.equal(hook.result.current.pending, false);
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
