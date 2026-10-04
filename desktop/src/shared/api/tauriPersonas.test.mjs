import assert from "node:assert/strict";
import test from "node:test";

import { fromRawPersona } from "./tauriPersonas.ts";
import * as personas from "./tauriPersonas.ts";

function nativeIPC(t, handler) {
  const calls = [];
  const previous = globalThis.window;
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async (command, args) => {
        calls.push({ command, args });
        return handler(command, args);
      },
    },
  };
  t.after(() => {
    if (previous === undefined) delete globalThis.window;
    else globalThis.window = previous;
  });
  return calls;
}

function rawPersona(overrides = {}) {
  return {
    id: "persona-1",
    display_name: "Team Analyst",
    avatar_url: null,
    system_prompt: "You are Team Analyst.",
    runtime: null,
    model: null,
    provider: null,
    name_pool: [],
    is_builtin: false,
    is_active: true,
    source_team: null,
    env_vars: {},
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    ...overrides,
  };
}

test("fromRawPersona maps source_team to sourceTeam", () => {
  const persona = fromRawPersona(rawPersona({ source_team: "team-research" }));

  assert.equal(persona.sourceTeam, "team-research");
});

test("fromRawPersona maps authored description and defaults absence to null", () => {
  assert.equal(
    fromRawPersona(rawPersona({ description: "A careful analyst." }))
      .description,
    "A careful analyst.",
  );
  assert.equal(fromRawPersona(rawPersona()).description, null);
});

test("raw_home_maps_pubkeys_and_capabilities", async (t) => {
  const home = {
    kind: "remote",
    label: "Office laptop",
    remoteInstancePubkeys: ["b".repeat(64), "c".repeat(64)],
  };
  const capabilities = {
    canCreateInstance: false,
    canDeleteDefinition: false,
    blockedReason: "definition_hosted_elsewhere",
  };
  nativeIPC(t, () => [
    rawPersona({
      share_across_devices: false,
      origin_device_id: "device-a",
      origin_device_label: "Office laptop",
      origin_released: false,
      home,
      capabilities,
      shared: true,
      respond_to: "allowlist",
      respond_to_allowlist: ["d".repeat(64)],
    }),
  ]);
  const [persona] = await personas.listPersonas();
  assert.deepEqual(persona.home, home);
  assert.deepEqual(persona.capabilities, capabilities);
  assert.equal(persona.shareAcrossDevices, false);
  assert.equal(persona.originDeviceId, "device-a");
  assert.equal(persona.originDeviceLabel, "Office laptop");
  assert.equal(persona.originReleased, false);
  assert.equal(persona.shared, true);
  assert.equal(persona.respondTo, "allowlist");
  assert.deepEqual(persona.respondToAllowlist, ["d".repeat(64)]);
});

test("raw mutation responses preserve absent projection and policy", async (t) => {
  nativeIPC(t, () => rawPersona());
  for (const persona of [
    await personas.createPersona({
      displayName: "New",
      systemPrompt: "Prompt",
    }),
    await personas.updatePersona({
      id: "persona-1",
      displayName: "Edit",
      systemPrompt: "Prompt",
    }),
  ]) {
    for (const field of [
      "home",
      "homeError",
      "capabilities",
      "shareAcrossDevices",
      "originDeviceId",
      "originDeviceLabel",
      "originReleased",
      "deviceHostBinding",
    ])
      assert.equal(field in persona, false, `${field} was invented`);
  }
});

test("pending_backfill_capabilities_cannot_create_or_delete", async (t) => {
  nativeIPC(t, () => [
    rawPersona({
      home: { kind: "unclaimed", label: null, remoteInstancePubkeys: [] },
      capabilities: {
        canCreateInstance: false,
        canDeleteDefinition: false,
        blockedReason: "device_home_sync_pending",
      },
    }),
    rawPersona({
      home: null,
      homeError: "evidence unavailable",
      capabilities: {
        canCreateInstance: false,
        canDeleteDefinition: false,
        blockedReason: "device_home_sync_failed",
      },
    }),
    rawPersona({
      share_across_devices: true,
      home: null,
      homeError: "evidence unavailable",
      capabilities: {
        canCreateInstance: true,
        canDeleteDefinition: true,
        blockedReason: null,
      },
    }),
  ]);
  const [pending, failed, shared] = await personas.listPersonas();
  assert.equal(pending.home.kind, "unclaimed");
  assert.deepEqual(pending.capabilities, {
    canCreateInstance: false,
    canDeleteDefinition: false,
    blockedReason: "device_home_sync_pending",
  });
  assert.equal(failed.home, null);
  assert.equal(failed.homeError, "evidence unavailable");
  assert.equal(failed.capabilities.blockedReason, "device_home_sync_failed");
  assert.equal(shared.home, null);
  assert.deepEqual(shared.capabilities, {
    canCreateInstance: true,
    canDeleteDefinition: true,
    blockedReason: null,
  });
});

test("create_payload_default_false", async (t) => {
  const calls = nativeIPC(t, () => rawPersona());
  const input = {
    displayName: "New",
    systemPrompt: "Prompt",
    behavior: { respondTo: "allowlist", respondToAllowlist: ["a".repeat(64)] },
    catalogSource: {
      ownerPubkey: "b".repeat(64),
      personaId: "catalog-persona",
    },
    originDeviceId: "forged",
    originDeviceLabel: "forged",
    originReleased: true,
    deviceHostBinding: "forged",
    home: { kind: "local" },
    capabilities: { canCreateInstance: true },
  };
  await personas.createPersona(input);
  await personas.createPersona({ ...input, shareAcrossDevices: true });
  await personas.createPersona({ ...input, shareAcrossDevices: false });
  assert.deepEqual(
    calls.map(({ args }) => args.input.shareAcrossDevices),
    [false, true, false],
  );
  for (const { command, args } of calls) {
    assert.equal(command, "create_persona");
    assert.deepEqual(args.input.behavior, input.behavior);
    assert.deepEqual(args.input.catalogSource, input.catalogSource);
    for (const field of [
      "originDeviceId",
      "originDeviceLabel",
      "originReleased",
      "deviceHostBinding",
      "home",
      "capabilities",
      "shared",
    ])
      assert.equal(field in args.input, false, `${field} entered create IPC`);
  }
});

test("edit_payload_does_not_change_policy", async (t) => {
  const calls = nativeIPC(t, (command) =>
    command === "update_persona_and_publish"
      ? { persona: rawPersona(), publicationStatus: "queued" }
      : rawPersona(),
  );
  const input = {
    id: "persona-1",
    displayName: "Edit",
    systemPrompt: "Prompt",
    behavior: { respondTo: "allowlist", respondToAllowlist: ["a".repeat(64)] },
    shareAcrossDevices: true,
    originDeviceId: "forged",
    originDeviceLabel: "forged",
    originReleased: true,
    deviceHostBinding: "forged",
  };
  await personas.updatePersona(input);
  const publication = await personas.updatePersonaAndPublish(input);
  assert.equal(publication.publicationStatus, "queued");
  for (const { args } of calls) {
    assert.deepEqual(args.input.behavior, input.behavior);
    for (const field of [
      "shareAcrossDevices",
      "originDeviceId",
      "originDeviceLabel",
      "originReleased",
      "deviceHostBinding",
    ])
      assert.equal(field in args.input, false, `${field} entered edit IPC`);
  }
});

test("sync IPC retains producer session, history manifest and rejection", async (t) => {
  const session = {
    token: "token",
    ownerPubkey: "owner",
    relayUrl: "wss://relay",
    workspaceGeneration: 7,
  };
  const calls = nativeIPC(t, (command) => {
    if (command === "begin_device_home_sync") return session;
    if (command === "hydrate_device_home_history")
      return { coveredEventIds: ["old", "new"] };
    if (command === "reconcile_inbound_persona_event") throw "apply failed";
  });
  assert.deepEqual(await personas.beginDeviceHomeSync(), session);
  assert.deepEqual(await personas.hydrateDeviceHomeHistory(session.token), {
    coveredEventIds: ["old", "new"],
  });
  await assert.rejects(
    personas.reconcileInboundPersonaEvent(
      "{}",
      session.relayUrl,
      session.token,
    ),
    /apply failed/,
  );
  await personas.invalidateDeviceHomeSync(session.token);
  await personas.finishDeviceHomeSync(session.token);
  assert.deepEqual(calls.slice(1), [
    { command: "hydrate_device_home_history", args: { sessionToken: "token" } },
    {
      command: "reconcile_inbound_persona_event",
      args: {
        eventJson: "{}",
        arrivalRelayUrl: "wss://relay",
        sessionToken: "token",
      },
    },
    { command: "invalidate_device_home_sync", args: { sessionToken: "token" } },
    { command: "finish_device_home_sync", args: { sessionToken: "token" } },
  ]);
});
