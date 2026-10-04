import assert from "node:assert/strict";
import test from "node:test";
import { fromRawPersona, createPersona } from "@/shared/api/tauriPersonas";
import {
  getDefinitionForAction,
  requireDefinitionCapability,
} from "./definitionCapabilities.ts";

const raw = (overrides = {}) => ({
  id: "definition",
  display_name: "Agent",
  avatar_url: null,
  system_prompt: "Prompt",
  is_builtin: false,
  created_at: "now",
  updated_at: "now",
  ...overrides,
});
const projected = (overrides = {}) => ({
  ...fromRawPersona(raw()),
  home: { kind: "unclaimed", label: null, remoteInstancePubkeys: [] },
  capabilities: {
    canCreateInstance: true,
    canDeleteDefinition: true,
    blockedReason: null,
  },
  ...overrides,
});

test("missing_projection_never_authorizes_start", () => {
  for (const action of ["createInstance", "deleteDefinition"]) {
    for (const persona of [
      fromRawPersona(raw()),
      projected({ home: undefined }),
      projected({ capabilities: undefined }),
      projected({ capabilities: {} }),
    ])
      assert.throws(
        () => requireDefinitionCapability(persona, action),
        /definition_capabilities_unavailable|definition_action_unavailable/,
      );
  }
});

test("capability checks use each backend action and retain pending failed and hosted reasons", () => {
  for (const reason of [
    "device_home_sync_pending",
    "device_home_sync_failed",
    "definition_hosted_elsewhere",
  ]) {
    const persona = projected({
      home: { kind: "remote", label: "Office", remoteInstancePubkeys: ["key"] },
      capabilities: {
        canCreateInstance: false,
        canDeleteDefinition: false,
        blockedReason: reason,
      },
    });
    for (const action of ["createInstance", "deleteDefinition"])
      assert.throws(
        () => requireDefinitionCapability(persona, action),
        (error) => error.code === reason && error.homeLabel === "Office",
      );
  }
  requireDefinitionCapability(
    projected({
      capabilities: {
        canCreateInstance: true,
        canDeleteDefinition: false,
        blockedReason: "delete refused",
      },
    }),
    "createInstance",
  );
  assert.throws(
    () =>
      requireDefinitionCapability(
        projected({
          capabilities: {
            canCreateInstance: true,
            canDeleteDefinition: false,
            blockedReason: "delete refused",
          },
        }),
        "deleteDefinition",
      ),
    /delete refused/,
  );
  requireDefinitionCapability(
    projected({
      capabilities: {
        canCreateInstance: false,
        canDeleteDefinition: true,
        blockedReason: "create refused",
      },
    }),
    "deleteDefinition",
  );
  assert.throws(
    () =>
      requireDefinitionCapability(
        projected({
          capabilities: {
            canCreateInstance: false,
            canDeleteDefinition: true,
            blockedReason: "create refused",
          },
        }),
        "createInstance",
      ),
    /create refused/,
  );
});

test("explicit backend sharing fast path permits unavailable home without client inference", () => {
  const shared = projected({
    shareAcrossDevices: true,
    home: null,
    homeError: "proof unavailable",
  });
  for (const action of ["createInstance", "deleteDefinition"])
    requireDefinitionCapability(shared, action);
  assert.throws(
    () =>
      requireDefinitionCapability(
        { ...shared, capabilities: undefined },
        "createInstance",
      ),
    /definition_capabilities_unavailable/,
  );
});

function ipc(t, handler) {
  const previous = globalThis.window;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: handler } };
  t.after(() => {
    if (previous === undefined) delete globalThis.window;
    else globalThis.window = previous;
  });
}

test("getter refresh after raw create observes backend refusal before any mutation", async (t) => {
  const calls = [];
  ipc(t, async (command) => {
    calls.push(command);
    if (command === "create_persona") return raw();
    if (command === "list_personas")
      return [
        raw({
          home: {
            kind: "remote",
            label: "Office",
            remoteInstancePubkeys: ["key"],
          },
          capabilities: {
            canCreateInstance: false,
            canDeleteDefinition: false,
            blockedReason: "definition_hosted_elsewhere",
          },
        }),
      ];
    assert.fail(`unauthorized mutation ${command}`);
  });
  const created = await createPersona({
    displayName: "Agent",
    systemPrompt: "Prompt",
  });
  assert.equal(created.home, undefined);
  await assert.rejects(
    getDefinitionForAction(created.id, "createInstance"),
    /definition_hosted_elsewhere.*Office/,
  );
  assert.deepEqual(calls, ["create_persona", "list_personas"]);
});

test("getter re-reads each action and propagates missing definition or failed list", async (t) => {
  let lists = 0;
  ipc(t, async (command) => {
    assert.equal(command, "list_personas");
    const unrelated = raw({
      id: "other-definition",
      home: { kind: "local", label: "Wrong", remoteInstancePubkeys: [] },
      capabilities: {
        canCreateInstance: true,
        canDeleteDefinition: true,
        blockedReason: null,
      },
    });
    if (++lists === 1)
      return [
        unrelated,
        raw({
          home: { kind: "local", label: "Here", remoteInstancePubkeys: [] },
          capabilities: {
            canCreateInstance: true,
            canDeleteDefinition: true,
            blockedReason: null,
          },
        }),
      ];
    if (lists === 2)
      return [
        raw({
          home: null,
          homeError: "unavailable",
          capabilities: {
            canCreateInstance: false,
            canDeleteDefinition: false,
            blockedReason: "device_home_sync_failed",
          },
        }),
      ];
    if (lists === 3) return [unrelated];
    throw "inventory unavailable";
  });
  const fresh = await getDefinitionForAction("definition", "createInstance");
  assert.equal(fresh.home.label, "Here");
  await assert.rejects(
    getDefinitionForAction("definition", "deleteDefinition"),
    /device_home_sync_failed/,
  );
  await assert.rejects(
    getDefinitionForAction("definition", "createInstance"),
    /definition_not_found/,
  );
  await assert.rejects(
    getDefinitionForAction("definition", "createInstance"),
    /inventory unavailable/,
  );
  assert.equal(lists, 4);
});

test("getter refuses a raw list projection without assuming unclaimed", async (t) => {
  ipc(t, async () => [raw()]);
  await assert.rejects(
    getDefinitionForAction("definition", "createInstance"),
    /definition_capabilities_unavailable/,
  );
});
