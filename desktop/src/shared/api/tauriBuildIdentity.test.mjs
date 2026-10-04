import assert from "node:assert/strict";
import { test } from "node:test";
import { forkBuildLabel, getForkBuildIdentity } from "./tauriBuildIdentity.ts";

test("About preserves upstream version and adds exact fork number and SHA", () => {
  const identity = {
    forkRevision: "1",
    commitSha: "a".repeat(40),
    baseTag: "desktop-v0.5.26",
  };
  assert.equal(forkBuildLabel(identity), `Fork 1 · ${identity.commitSha}`);
  assert.equal(forkBuildLabel(null), null);
});

test("getter maps actual native command and absence without runtime SHA inference", async () => {
  const commands = [];
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd) => {
        commands.push(cmd);
        return {
          fork_revision: "1",
          commit_sha: "a".repeat(40),
          base_tag: "desktop-v0.5.26",
        };
      },
    },
  };
  try {
    assert.deepEqual(await getForkBuildIdentity(), {
      forkRevision: "1",
      commitSha: "a".repeat(40),
      baseTag: "desktop-v0.5.26",
    });
    assert.deepEqual(commands, ["get_fork_build_identity"]);
    window.__TAURI_INTERNALS__.invoke = async () => null;
    assert.equal(await getForkBuildIdentity(), null);
  } finally {
    delete globalThis.window;
  }
});
