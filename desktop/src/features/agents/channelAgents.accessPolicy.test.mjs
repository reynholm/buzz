import assert from "node:assert/strict";
import test from "node:test";

import { applyReusableAgentAccessPolicy } from "./channelAgents.ts";

const AGENT_PUBKEY = "a".repeat(64);
const ALLOWED_PUBKEY = "b".repeat(64);

// `wrote` is load-bearing: the message-send path (useMentionSendFlow) uses it
// to decide whether an awaited relay round-trip separated its pre-side-effect
// mention-authorization pass from the publish, and therefore whether it must
// revalidate at the publish boundary (#5681). These tests pin the flag against
// the relay write itself, not against the identity of the returned record.

function rawAgent(overrides = {}) {
  return {
    pubkey: AGENT_PUBKEY,
    name: "fizz",
    persona_id: null,
    relay_url: "wss://relay.example",
    acp_command: "buzz-acp",
    agent_command: "goose",
    agent_args: [],
    mcp_command: "",
    turn_timeout_seconds: 0,
    idle_timeout_seconds: 0,
    max_turn_duration_seconds: 0,
    parallelism: 1,
    system_prompt: null,
    model: null,
    status: "running",
    pid: null,
    created_at: "2026-01-15T00:00:00Z",
    updated_at: "2026-01-15T00:00:00Z",
    last_started_at: null,
    last_stopped_at: null,
    last_exit_code: null,
    last_error: null,
    log_path: null,
    start_on_app_launch: false,
    backend: { type: "local" },
    backend_agent_id: null,
    respond_to: "owner-only",
    respond_to_allowlist: [],
    ...overrides,
  };
}

function managedAgent(overrides = {}) {
  return {
    pubkey: AGENT_PUBKEY,
    name: "fizz",
    respondTo: "owner-only",
    respondToAllowlist: [],
    ...overrides,
  };
}

function installTauriInvoke(handler) {
  const prior = globalThis.window;
  globalThis.window ??= {};
  window.__TAURI_INTERNALS__ = { invoke: handler };
  return () => {
    globalThis.window = prior;
  };
}

test("a matching access policy reports no write and returns the agent untouched", async (t) => {
  const calls = [];
  t.after(
    installTauriInvoke((command, args) => {
      calls.push([command, args]);
      return Promise.resolve(null);
    }),
  );

  const agent = managedAgent();
  const result = await applyReusableAgentAccessPolicy(agent, {});

  assert.equal(result.wrote, false);
  assert.equal(result.agent, agent);
  assert.deepEqual(calls, []);
});

test("a diverging access policy reports the write and returns the updated agent", async (t) => {
  const calls = [];
  t.after(
    installTauriInvoke((command, args) => {
      calls.push([command, args]);
      return Promise.resolve({
        agent: rawAgent({
          respond_to: "allowlist",
          respond_to_allowlist: [ALLOWED_PUBKEY],
        }),
        profile_sync_error: null,
      });
    }),
  );

  const agent = managedAgent();
  const result = await applyReusableAgentAccessPolicy(agent, {
    respondTo: "allowlist",
    respondToAllowlist: [ALLOWED_PUBKEY],
  });

  assert.equal(result.wrote, true);
  assert.equal(result.agent.respondTo, "allowlist");
  assert.deepEqual(result.agent.respondToAllowlist, [ALLOWED_PUBKEY]);
  assert.deepEqual(calls, [
    [
      "update_managed_agent",
      {
        input: {
          pubkey: AGENT_PUBKEY,
          respondTo: "allowlist",
          respondToAllowlist: [ALLOWED_PUBKEY],
        },
      },
    ],
  ]);
});

test("the write is reported even when the update hands back an unchanged record", async (t) => {
  // Callers must not re-derive the write by comparing the returned record
  // against the one they passed in — a backend that normalizes the policy
  // away, or a cache layer that mutates in place and hands the caller's own
  // object back, still wrote to the relay. Under such a comparison the send
  // path would silently skip the publish-boundary revalidation.
  let invoked = 0;
  t.after(
    installTauriInvoke(() => {
      invoked += 1;
      return Promise.resolve({
        agent: rawAgent(),
        profile_sync_error: null,
      });
    }),
  );

  const agent = managedAgent();
  const result = await applyReusableAgentAccessPolicy(agent, {
    respondTo: "anyone",
  });

  assert.equal(invoked, 1);
  assert.equal(result.wrote, true);
  assert.equal(result.agent.respondTo, agent.respondTo);
  assert.deepEqual(result.agent.respondToAllowlist, agent.respondToAllowlist);
});

const channelActions = await import("./channelAgents.ts");
const { fromRawManagedAgent } = await import("../../shared/api/tauri.ts");
const { fromRawPersona } = await import("../../shared/api/tauriPersonas.ts");
const { canReuseManagedAgentOnDevice } = await import(
  "./lib/definitionCapabilities.ts"
);
const runtime = {
  id: "goose",
  label: "Goose",
  command: "goose",
  defaultArgs: [],
  mcpCommand: "",
};
function definition(allowed = true, reason = null) {
  return {
    id: "persona",
    display_name: "Agent",
    system_prompt: "prompt",
    env_vars: {},
    is_builtin: false,
    is_active: true,
    home: allowed
      ? { kind: "local", label: "Here", instancePubkeys: [] }
      : { kind: "remote", label: "Laptop A", instancePubkeys: [] },
    capabilities: {
      canCreateInstance: allowed,
      canDeleteDefinition: allowed,
      blockedReason: reason,
    },
    respond_to: "allowlist",
    respond_to_allowlist: [ALLOWED_PUBKEY],
  };
}
function installChannelFixture(t, options = {}) {
  const { allowed = true, reason = null, instances = true } = options;
  const canStart = "canStart" in options ? options.canStart : true;
  const calls = [];
  let raw = rawAgent({
    name: "goose",
    persona_id: "persona",
    can_start_on_device: canStart,
  });
  t.after(
    installTauriInvoke(async (command, args) => {
      calls.push([command, args]);
      if (command === "list_personas") return [definition(allowed, reason)];
      if (command === "list_managed_agents") return instances ? [raw] : [];
      if (command === "get_channel_members") return { members: [] };
      if (command === "add_channel_members")
        return { added: args.pubkeys, errors: [] };
      if (command === "create_managed_agent")
        return { agent: raw, spawn_error: null, profile_sync_error: null };
      if (command === "start_managed_agent") return raw;
      if (command === "update_managed_agent") {
        raw = {
          ...raw,
          respond_to: args.input.respondTo,
          respond_to_allowlist: args.input.respondToAllowlist ?? [],
        };
        return { agent: raw, profile_sync_error: null };
      }
      throw new Error(`Unexpected IPC ${command}`);
    }),
  );
  const agent = fromRawManagedAgent(raw);
  return {
    calls,
    agent,
    context: {
      managedAgents: [agent],
      channelMemberPubkeys: new Set(),
      personas: [fromRawPersona(definition())],
    },
  };
}
function mutations(calls) {
  return calls.filter(([name]) =>
    [
      "add_channel_members",
      "create_managed_agent",
      "start_managed_agent",
      "update_managed_agent",
    ].includes(name),
  );
}
test("instance eligibility maps backend boolean exactly; undefined is fail closed", () => {
  for (const [wire, want] of [
    [true, true],
    [false, false],
    [undefined, false],
  ]) {
    const agent = fromRawManagedAgent(rawAgent({ can_start_on_device: wire }));
    assert.equal(agent.canStartOnDevice, wire);
    assert.equal(canReuseManagedAgentOnDevice?.(agent.canStartOnDevice), want);
  }
});
for (const reason of [
  "definition_hosted_elsewhere",
  "device_home_sync_pending",
  "device_home_sync_failed",
]) {
  for (const mode of [
    "reuse",
    "force-create",
    "direct-provision",
    "direct-attach",
  ]) {
    test(`channel capability refuses ${mode} before every mutation (${reason})`, async (t) => {
      const f = installChannelFixture(t, { allowed: false, reason });
      const input = {
        runtime,
        name: "Agent",
        personaId: "persona",
        respondTo: "anyone",
        forceNewInstance: mode === "force-create",
      };
      await assert.rejects(
        () =>
          mode === "direct-attach"
            ? channelActions.attachManagedAgentToChannel("channel", {
                agent: f.agent,
                ensureRunning: false,
              })
            : mode === "direct-provision"
              ? channelActions.provisionChannelManagedAgent(input, f.context)
              : channelActions.createChannelManagedAgent(
                  "channel",
                  input,
                  f.context,
                ),
        new RegExp(reason),
      );
      assert.deepEqual(mutations(f.calls), []);
    });
  }
}
for (const mode of ["reuse", "direct-attach", "preset"]) {
  test(`channel_reuse_never_attaches_copied_instance (${mode})`, async (t) => {
    const f = installChannelFixture(t, { canStart: false });
    await assert.rejects(
      () =>
        mode === "direct-attach"
          ? channelActions.attachManagedAgentToChannel("channel", {
              agent: f.agent,
              ensureRunning: false,
              detachedStart: () => assert.fail("must not queue copied start"),
            })
          : mode === "preset"
            ? channelActions.ensureChannelAgentPresetInChannel("channel", {
                runtime,
              })
            : channelActions.createChannelManagedAgent(
                "channel",
                { runtime, name: "Agent", personaId: "persona" },
                f.context,
              ),
      /instance_not_runnable_on_device/,
    );
    assert.deepEqual(mutations(f.calls), []);
  });
}
for (const [mode, list] of [
  ["owner-only", []],
  ["allowlist", [ALLOWED_PUBKEY]],
  ["anyone", []],
]) {
  test(`reuse_preserves_access_policy ${mode}`, async (t) => {
    const f = installChannelFixture(t);
    const result = await channelActions.createChannelManagedAgent(
      "channel",
      {
        runtime,
        name: "Agent",
        personaId: "persona",
        respondTo: mode,
        respondToAllowlist: list,
      },
      f.context,
    );
    assert.equal(result.created, false);
    assert.equal(result.agent.pubkey, AGENT_PUBKEY);
    assert.equal(result.agent.respondTo, mode);
    assert.deepEqual(result.agent.respondToAllowlist, list);
    const access = f.calls.filter(([name]) => name === "update_managed_agent");
    assert.deepEqual(
      access,
      mode === "owner-only"
        ? []
        : [
            [
              "update_managed_agent",
              {
                input: {
                  pubkey: AGENT_PUBKEY,
                  respondTo: mode,
                  respondToAllowlist: list,
                },
              },
            ],
          ],
    );
    assert.equal(
      f.calls.filter(([name]) => name === "add_channel_members").length,
      1,
    );
    assert.equal(
      f.calls.filter(([name]) => name === "create_managed_agent").length,
      0,
    );
  });
}
for (const canStart of [false, undefined]) {
  test(`channel refresh refuses stale allowed instance before access policy (${canStart})`, async (t) => {
    const f = installChannelFixture(t, { canStart });
    f.context.managedAgents = [{ ...f.agent, canStartOnDevice: true }];
    await assert.rejects(
      () =>
        channelActions.createChannelManagedAgent(
          "channel",
          { runtime, name: "Agent", personaId: "persona", respondTo: "anyone" },
          f.context,
        ),
      /instance_not_runnable_on_device/,
    );
    assert.deepEqual(mutations(f.calls), []);
    assert.ok(f.calls.some(([name]) => name === "list_managed_agents"));
  });
}
test("a vanished exact instance cannot attach a cached identity", async (t) => {
  const f = installChannelFixture(t, { instances: false });
  await assert.rejects(
    () =>
      channelActions.attachManagedAgentToChannel("channel", { agent: f.agent }),
    /instance_not_found/,
  );
  assert.deepEqual(mutations(f.calls), []);
});
test("generic reuse also refuses missing exact-instance authority before access write", async (t) => {
  const f = installChannelFixture(t, { canStart: undefined });
  window.__TAURI_INTERNALS__.invoke = async (command, args) => {
    f.calls.push([command, args]);
    if (command === "list_managed_agents")
      return [rawAgent({ persona_id: null })];
    if (command === "update_managed_agent")
      return { agent: rawAgent(), profile_sync_error: null };
    throw new Error(`Unexpected IPC ${command}`);
  };
  await assert.rejects(
    () =>
      channelActions.provisionChannelManagedAgent(
        { runtime, name: "Agent", respondTo: "anyone" },
        f.context,
      ),
    /instance_not_runnable_on_device/,
  );
  assert.deepEqual(mutations(f.calls), []);
});
