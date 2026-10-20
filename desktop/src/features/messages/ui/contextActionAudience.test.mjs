import assert from "node:assert/strict";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";
import {
  resetPersistentAgentAudienceStore,
  usePersistentAgentAudience,
  excludePersistentAgentAudienceMember,
} from "../lib/persistentAgentAudience.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
const agent = "a".repeat(64);
before(() =>
  Object.assign(globalThis, {
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
    window: dom.window,
  }),
);
afterEach(async () => {
  const { cleanup } = await import("@testing-library/react");
  cleanup();
  resetPersistentAgentAudienceStore();
});
after(() => dom.window.close());

async function setup() {
  const { renderHook } = await import("@testing-library/react");
  const { useAgentAddressLockPicker } = await import(
    "./useAgentAddressLockPicker.ts"
  );
  const edits = [],
    bindings = [],
    pulses = [];
  const hook = renderHook(
    ({ scope }) => {
      const audience = usePersistentAgentAudience(scope);
      const picker = useAgentAddressLockPicker({
        audience,
        audienceScope: scope,
        applyAutocompleteEdit: (edit) => edits.push(edit),
        mentions: {
          getDraftMentionRefs: () => [],
          getMentionDisplayName: () => "Agent A",
          registerMentionPubkey: (...args) => {
            bindings.push(args);
            return "Agent A";
          },
        },
        onPulseAddressLock: (pubkey) => pulses.push(pubkey),
        richText: {
          getPlainTextAndCursor: () => ({ text: "Keep my draft", cursor: 3 }),
        },
      });
      return { audience, picker };
    },
    { initialProps: { scope: "thread-one" } },
  );
  return { ...hook, edits, bindings, pulses };
}

test("a context action pins only its target without editing, binding or excluding it on the next keystroke", async () => {
  const { act } = await import("@testing-library/react");
  const hook = await setup();
  const commit = hook.result.current.picker.prepareAgentPin(agent);
  assert.deepEqual(hook.result.current.audience.pubkeys, []);
  act(commit);
  assert.deepEqual(hook.result.current.audience.pubkeys, [agent]);
  assert.deepEqual(hook.pulses, [agent]);
  act(() => hook.result.current.picker.restoreAddressedAgentMentions());
  act(() =>
    hook.result.current.picker.syncAddressedAgentsFromText("Keep my draft!"),
  );
  assert.deepEqual(hook.result.current.audience.pubkeys, [agent]);
  assert.deepEqual(hook.edits, []);
  assert.deepEqual(hook.bindings, []);
  // A normal send clears the authored draft and explicitly restores its audience.
  act(() => hook.result.current.picker.restoreAddressedAgentMentions([agent]));
  assert.equal(hook.edits[0].insertText, "@Agent A ");
});

for (const change of ["audience", "scope round trip"]) {
  test(`late context publication cannot override ${change}`, async () => {
    const { act } = await import("@testing-library/react");
    const hook = await setup();
    const commit = hook.result.current.picker.prepareAgentPin(agent);
    if (change === "audience") {
      act(() => excludePersistentAgentAudienceMember("thread-one", agent));
    } else {
      hook.rerender({ scope: "thread-two" });
      hook.rerender({ scope: "thread-one" });
    }
    act(commit);
    assert.deepEqual(hook.result.current.audience.pubkeys, []);
    assert.deepEqual(hook.edits, []);
    assert.deepEqual(hook.pulses, []);
  });
}
