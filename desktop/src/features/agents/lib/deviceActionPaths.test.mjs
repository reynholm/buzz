import assert from "node:assert/strict";
import fs from "node:fs";
import { registerHooks } from "node:module";
import path from "node:path";
import { after, afterEach, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";
import ts from "typescript";

// Only presentation/navigation/huddle adapters are replaced. All owning
// action handlers, React Query mutations, fresh getter and native IPC wrappers
// run unchanged. The panel's real callbacks are captured at its render boundary.
let summaryProps, dialogProps;
globalThis.__deviceSummary = (props) => {
  summaryProps = props;
  return null;
};
globalThis.__deviceDialogs = (props) => {
  dialogProps = props;
  return null;
};
const replacements = new Map([
  [
    "/shared/layout/AuxiliaryPanelBody.tsx",
    "export const AuxiliaryPanelBody = props => props.children;",
  ],
  [
    "/features/agents/ui/AgentSessionTranscriptList.tsx",
    "export const AgentSessionTranscriptList = () => null;",
  ],
  [
    "/app/navigation/useAppNavigation.ts",
    "export const useAppNavigation = () => ({goChannel() {}});",
  ],
  [
    "/features/huddle/index.ts",
    "export const useHuddle = () => ({isStarting:false,startHuddle:async()=>{}});",
  ],
  [
    "/features/profile/ui/UserProfilePanelSections.tsx",
    "export const ProfileSummaryView = props => globalThis.__deviceSummary(props); export const AgentInstructionsFocusedView = () => null;",
  ],
  [
    "/features/profile/ui/UserProfilePanelFrame.tsx",
    'import {createElement, Fragment} from "react"; export const UserProfilePanelFrame = props => createElement(Fragment,null,props.profileBody,props.personaDialogs);',
  ],
  [
    "/features/profile/ui/UserProfilePersonaDialogs.tsx",
    "export const UserProfilePersonaDialogs = props => globalThis.__deviceDialogs(props); export const useCardMint = () => ({target:null,create(){},close(){}});",
  ],
]);
const loader = registerHooks({
  load(url, context, nextLoad) {
    for (const [suffix, source] of replacements) {
      if (url.endsWith(suffix))
        return { format: "module", shortCircuit: true, source };
    }
    return nextLoad(url, context);
  },
});
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
const PK = "a".repeat(64),
  OWNER = "b".repeat(64),
  ALLOWED = "c".repeat(64);
const runtime = {
  id: "goose",
  label: "Goose",
  command: "goose",
  defaultArgs: [],
  mcpCommand: "",
  availability: "available",
  avatarUrl: null,
};
const channel = {
  id: "channel",
  name: "agents",
  isMember: true,
  memberPubkeys: [PK],
  participantPubkeys: [],
  visibility: "open",
};
function rawPersona(overrides = {}) {
  return {
    id: "persona",
    display_name: "Agent",
    system_prompt: "prompt",
    model: null,
    provider: null,
    runtime: "goose",
    avatar_url: null,
    env_vars: {},
    is_builtin: false,
    is_active: true,
    shared: false,
    respond_to: "allowlist",
    respond_to_allowlist: [ALLOWED],
    home: { kind: "local", label: "Here", instancePubkeys: [] },
    capabilities: {
      canCreateInstance: true,
      canDeleteDefinition: true,
      blockedReason: null,
    },
    ...overrides,
  };
}
function rawAgent(overrides = {}) {
  return {
    pubkey: PK,
    name: "Agent",
    persona_id: "persona",
    team_id: null,
    runtime: null,
    relay_url: "wss://relay.example",
    acp_command: "buzz-acp",
    agent_command: "goose",
    agent_args: [],
    mcp_command: "",
    parallelism: 1,
    system_prompt: "prompt",
    model: null,
    provider: null,
    status: "stopped",
    pid: null,
    created_at: "now",
    updated_at: "now",
    start_on_app_launch: false,
    backend: { type: "local" },
    backend_agent_id: null,
    respond_to: "owner-only",
    respond_to_allowlist: [],
    can_start_on_device: true,
    ...overrides,
  };
}
function blocked(reason = "definition_hosted_elsewhere", overrides = {}) {
  return rawPersona({
    home: { kind: "remote", label: "Laptop A", instancePubkeys: [PK] },
    capabilities: {
      canCreateInstance: false,
      canDeleteDefinition: false,
      blockedReason: reason,
    },
    ...overrides,
  });
}
let act,
  render,
  cleanup,
  waitFor,
  React,
  QueryClient,
  QueryClientProvider,
  CommunitiesProvider;
let useManagedAgentActions,
  usePersonaActions,
  useTeamActions,
  useAgentManagement,
  UserProfilePanel;
let channelAgents,
  welcomeGuide,
  fromRawPersona,
  fromRawManagedAgent,
  builder,
  observer,
  relayClient,
  toast;
let clients = [],
  commands = [],
  personas = [],
  agents = [],
  handlers = new Map(),
  notices = [],
  errors = [],
  toastErrors = [],
  restoreRelay,
  restoreToast;
before(async () => {
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    localStorage: dom.window.localStorage,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: dom.window.navigator,
  });
  dom.window.matchMedia = () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  });
  dom.window.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      commands.push([command, args]);
      if (handlers.has(command)) return handlers.get(command)(args);
      switch (command) {
        case "list_personas":
          return personas;
        case "list_managed_agents":
          return agents;
        case "create_persona":
          return rawPersona({
            id: "created",
            ...args.input,
            display_name: args.input.displayName,
            share_across_devices: args.input.shareAcrossDevices,
            home: undefined,
            capabilities: undefined,
          });
        case "create_managed_agent":
          return {
            agent: rawAgent({ persona_id: args.input.personaId ?? null }),
            spawn_error: null,
            profile_sync_error: null,
            private_key_nsec: "fixture",
          };
        case "update_managed_agent":
          return {
            agent: rawAgent({
              respond_to: args.input.respondTo,
              respond_to_allowlist: args.input.respondToAllowlist ?? [],
            }),
            profile_sync_error: null,
          };
        case "start_managed_agent":
          return rawAgent({ status: "running" });
        case "add_channel_members":
          return { added: args.pubkeys, errors: [] };
        case "get_channel_members":
          return { members: [] };
        case "list_available_acp_runtimes":
        case "discover_acp_runtimes":
        case "discover_acp_providers":
          return [runtime];
        case "get_global_agent_config":
          return { env_vars: {}, preferred_runtime: "goose" };
        case "agent_access_owner_only":
          return false;
        case "reconcile_managed_agent_runtimes":
          return [];
        case "get_channels":
          return [];
        case "get_relay_agents":
        case "list_teams":
        case "get_contact_list":
          return [];
        case "get_identity":
          return { pubkey: OWNER };
        case "get_presence":
        case "get_user_status":
          return {};
        case "plugin:event|listen":
          return 1;
        case "plugin:event|unlisten":
        case "delete_persona":
        case "delete_team":
          return null;
        case "set_persona_active":
          return rawPersona({ id: args.id, is_active: args.active });
        case "update_persona":
          return rawPersona({ display_name: args.input.displayName });
        case "list_archived_identities":
          return { archived: [], readOnly: false };
        default:
          throw new Error(`Unexpected fixture IPC ${command}`);
      }
    },
    transformCallback: () => 1,
  };
  ({ act, render, cleanup, waitFor } = await import("@testing-library/react"));
  React = await import("react");
  ({ QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  ));
  ({ CommunitiesProvider } = await import(
    "../../communities/useCommunities.tsx"
  ));
  ({ useManagedAgentActions } = await import(
    "../ui/useManagedAgentActions.ts"
  ));
  ({ usePersonaActions } = await import("../ui/usePersonaActions.ts"));
  ({ useTeamActions } = await import("../ui/useTeamActions.ts"));
  ({ useAgentManagement } = await import("../useAgentManagement.ts"));
  ({ UserProfilePanel } = await import(
    "../../profile/ui/UserProfilePanel.tsx"
  ));
  channelAgents = await import("../channelAgents.ts");
  welcomeGuide = await import("../../onboarding/welcomeGuide.ts");
  ({ fromRawPersona } = await import("../../../shared/api/tauriPersonas.ts"));
  ({ fromRawManagedAgent } = await import("../../../shared/api/tauri.ts"));
  ({ buildInstanceInputForDefinition: builder } = await import(
    "./instanceInputForDefinition.ts"
  ));
  observer = await import("../observerRelayStore.ts");
  ({ relayClient } = await import("../../../shared/api/relayClient.ts"));
  restoreRelay = {
    getConnectionState: relayClient.getConnectionState,
    subscribeToConnectionState: relayClient.subscribeToConnectionState,
  };
  relayClient.getConnectionState = () => "disconnected";
  relayClient.subscribeToConnectionState = () => () => {};
  ({ toast } = await import("sonner"));
  restoreToast = {
    error: toast.error,
    success: toast.success,
    warning: toast.warning,
  };
  toast.error = (message) => toastErrors.push(message);
  toast.success = () => {};
  toast.warning = () => {};
});
afterEach(() => {
  cleanup();
  for (const client of clients) {
    client.cancelQueries();
    client.clear();
  }
  clients = [];
  observer.resetAgentObserverStore();
});
after(() => {
  Object.assign(relayClient, restoreRelay);
  Object.assign(toast, restoreToast);
  loader.deregister();
  dom.window.close();
  delete globalThis.__deviceSummary;
  delete globalThis.__deviceDialogs;
});
function setup(rows = [blocked()], instances = []) {
  commands = [];
  personas = rows;
  agents = instances;
  handlers = new Map();
  notices = [];
  errors = [];
  toastErrors = [];
  summaryProps = null;
  dialogProps = null;
  dom.window.confirm = () => true;
}
function mount(owner, cached = rawPersona()) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: Infinity },
      mutations: { retry: false, gcTime: 0 },
    },
  });
  clients.push(client);
  for (const [key, value] of [
    ["personas", [fromRawPersona(cached)]],
    ["managed-agents", agents.map(fromRawManagedAgent)],
    ["relay-agents", []],
    ["channels", [channel]],
    ["globalAgentConfig", { env_vars: {}, preferred_runtime: "goose" }],
    ["acp-runtimes", [runtime]],
    ["available-acp-runtimes", [runtime]],
    ["identity", { pubkey: OWNER }],
  ])
    client.setQueryData([key], value);
  let current;
  function ManagedSurface() {
    current = useManagedAgentActions();
    return null;
  }
  function PersonaSurface() {
    current = usePersonaActions();
    return null;
  }
  function ManagementSurface() {
    current = useAgentManagement();
    return null;
  }
  function TeamSurface() {
    current = useTeamActions(
      {
        setActionNoticeMessage: (v) => notices.push(v),
        setActionErrorMessage: (v) => errors.push(v),
      },
      { refetchManagedAgents() {}, refetchRelayAgents() {} },
    );
    return null;
  }
  function ProfileSurface() {
    return React.createElement(UserProfilePanel, {
      persona: fromRawPersona(cached),
      currentPubkey: OWNER,
      onClose() {},
    });
  }
  const Surface = {
    managed: ManagedSurface,
    persona: PersonaSurface,
    management: ManagementSurface,
    team: TeamSurface,
    profile: ProfileSurface,
  }[owner];
  render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(
        CommunitiesProvider,
        null,
        React.createElement(Surface),
      ),
    ),
  );
  return { current: () => current, client };
}
function effects() {
  return commands.filter(([name]) =>
    [
      "create_managed_agent",
      "start_managed_agent",
      "update_managed_agent",
      "add_channel_members",
      "delete_persona",
      "delete_team",
      "set_persona_active",
    ].includes(name),
  );
}
async function requestManagement(surface) {
  await act(async () =>
    observer._testProcessLiveObserverEvents(PK, [
      {
        kind: "test",
        timestamp: new Date().toISOString(),
        seq: 1,
        payload: {
          type: "agent_management_request",
          requestId: "request",
          action: "create",
          request: {
            channelId: "channel",
            displayName: "Agent",
            systemPrompt: "prompt",
          },
        },
      },
    ]),
  );
  await waitFor(() =>
    assert.equal(surface.current().request?.action, "create"),
  );
}
const creationInput = {
  displayName: "Agent",
  systemPrompt: "prompt",
  runtime: "goose",
  behavior: { respondTo: "allowlist", respondToAllowlist: [ALLOWED] },
};
async function runOwner(owner, row) {
  const surface = mount(owner, rawPersona());
  if (owner === "managed")
    await act(async () =>
      surface.current().handleStartPersona(fromRawPersona(rawPersona())),
    );
  if (owner === "persona")
    await act(async () =>
      surface.current().handleSubmit(creationInput, "definition_start"),
    );
  if (owner === "management") {
    await requestManagement(surface);
    await act(async () =>
      surface.current().submitCreate(creationInput, "definition_start", null),
    );
  }
  if (owner === "profile") {
    await waitFor(() =>
      assert.equal(typeof summaryProps?.handleInstantiateAgent, "function"),
    );
    await act(async () => summaryProps.handleInstantiateAgent());
  }
  const reason =
    owner === "managed"
      ? surface.current().actionErrorMessage
      : owner === "persona"
        ? surface.current().personaErrorMessage
        : owner === "management"
          ? surface.current().error
          : toastErrors.at(-1);
  assert.match(
    reason ?? "",
    new RegExp(
      row.capabilities?.blockedReason ?? "definition_capabilities_unavailable",
    ),
  );
  assert.deepEqual(
    effects(),
    [],
    `${owner} must refuse before native mutations`,
  );
  assert.ok(
    commands.some(([name]) => name === "list_personas"),
    "must refresh definition instead of trusting cache",
  );
}
for (const owner of ["managed", "persona", "management", "profile"]) {
  for (const reason of [
    "definition_hosted_elsewhere",
    "device_home_sync_pending",
    "device_home_sync_failed",
    "missing",
  ]) {
    test(`${owner}: all_builder_callers_observe_remote_refusal / pending_history_blocks_every_creation_path (${reason})`, async () => {
      const row =
        reason === "missing"
          ? rawPersona({ home: undefined, capabilities: undefined })
          : blocked(reason);
      setup(
        [row],
        owner === "management" ? [rawAgent({ persona_id: null })] : [],
      );
      if (owner === "persona" || owner === "management")
        personas = [{ ...row, id: "created" }];
      await runOwner(owner, row);
    });
  }
}
for (const owner of ["persona", "management"]) {
  test(`${owner}: saved definition refusal closes creation with Start recovery and no duplicate retry`, async () => {
    setup(
      [blocked("device_home_sync_pending", { id: "created" })],
      owner === "management" ? [rawAgent({ persona_id: null })] : [],
    );
    const surface = mount(owner, rawPersona());
    if (owner === "management") await requestManagement(surface);
    else
      await act(() =>
        surface.current().openDuplicate(fromRawPersona(rawPersona())),
      );
    let result;
    await act(async () => {
      result =
        owner === "persona"
          ? await surface
              .current()
              .handleSubmit(creationInput, "definition_start")
          : await surface
              .current()
              .submitCreate(creationInput, "definition_start", null);
    });
    const message =
      owner === "persona"
        ? surface.current().personaErrorMessage
        : surface.current().error;
    assert.match(message, /saved/);
    assert.match(message, /Start/);
    assert.match(message, /device_home_sync_pending/);
    assert.deepEqual(effects(), []);
    assert.equal(
      commands.filter(([cmd]) => cmd === "create_persona").length,
      1,
    );
    if (owner === "persona") {
      assert.equal(
        result,
        true,
        "saved definition should dismiss Create instead of permitting a duplicate retry",
      );
      assert.equal(surface.current().personaDialogState, null);
    } else {
      assert.equal(surface.current().request, null);
      await act(async () =>
        surface.current().submitCreate(creationInput, "definition_start", null),
      );
      assert.equal(
        commands.filter(([cmd]) => cmd === "create_persona").length,
        1,
      );
    }
  });
}
test("persona: native refusal after saved definition keeps Start recovery", async () => {
  setup([rawPersona({ id: "created" })], []);
  handlers.set("create_managed_agent", () => {
    throw new Error("definition_hosted_elsewhere");
  });
  const surface = mount("persona", rawPersona());
  await act(() =>
    surface.current().openDuplicate(fromRawPersona(rawPersona())),
  );
  let result;
  await act(async () => {
    result = await surface
      .current()
      .handleSubmit(creationInput, "definition_start");
  });
  assert.equal(result, true);
  assert.match(surface.current().personaErrorMessage, /saved/);
  assert.match(surface.current().personaErrorMessage, /Start/);
  assert.match(
    surface.current().personaErrorMessage,
    /definition_hosted_elsewhere/,
  );
  assert.equal(surface.current().personaDialogState, null);
  assert.equal(commands.filter(([cmd]) => cmd === "create_persona").length, 1);
  assert.equal(
    commands.filter(([cmd]) => cmd === "start_managed_agent").length,
    0,
  );
});
for (const selected of [false, true])
  for (const owner of ["persona", "management"]) {
    test(`${owner}: new dialog creates definition first with selected policy ${selected}, exact linkage and access`, async () => {
      setup(
        [
          rawPersona({
            id: "created",
            home: selected ? null : rawPersona().home,
          }),
        ],
        owner === "management" ? [rawAgent({ persona_id: null })] : [],
      );
      const surface = mount(owner);
      if (owner === "management") await requestManagement(surface);
      await act(async () =>
        owner === "persona"
          ? surface
              .current()
              .handleSubmit(
                { ...creationInput, shareAcrossDevices: selected },
                "definition_start",
              )
          : surface
              .current()
              .submitCreate(
                { ...creationInput, shareAcrossDevices: selected },
                "definition_start",
                null,
              ),
      );
      const createDefinition = commands.find(
        ([name]) => name === "create_persona",
      );
      const createInstance = commands.find(
        ([name]) => name === "create_managed_agent",
      );
      assert.ok(
        createInstance,
        "backend-authorized definition must create instance",
      );
      assert.equal(createDefinition[1].input.shareAcrossDevices, selected);
      assert.equal(createDefinition[1].input.behavior.respondTo, "allowlist");
      assert.deepEqual(createDefinition[1].input.behavior.respondToAllowlist, [
        ALLOWED,
      ]);
      assert.equal(createInstance[1].input.personaId, "created");
      const readIndex = commands.findIndex(
        ([name]) => name === "list_personas",
      );
      assert.ok(
        commands.indexOf(createDefinition) < readIndex &&
          readIndex < commands.indexOf(createInstance),
      );
    });
  }
test("builder refuses before avatar upload even with provider intent", async () => {
  setup();
  const uploads = [];
  await assert.rejects(() =>
    builder(
      fromRawPersona(blocked()),
      runtime,
      async () => {
        uploads.push("upload");
      },
      { type: "provider", id: "host", config: {} },
    ),
  );
  assert.deepEqual(uploads, []);
});
for (const reason of [
  "definition_hosted_elsewhere",
  "device_home_sync_pending",
  "device_home_sync_failed",
]) {
  test(`onboarding_and_team_skip_with_reason (${reason})`, async () => {
    const rows = welcomeGuide.WELCOME_TEAM_STARTERS.map((starter) =>
      blocked(reason, {
        id: starter.personaId,
        is_builtin: true,
        is_active: false,
      }),
    );
    setup(rows);
    await assert.rejects(
      () => welcomeGuide.ensureWelcomeTeam("welcome", "wss://relay.example"),
      new RegExp(reason),
    );
    assert.deepEqual(effects(), []);
    commands = [];
    const result = await channelAgents.createChannelManagedAgents("channel", [
      { runtime, name: "Fizz", personaId: rows[0].id },
    ]);
    assert.equal(result.successes.length, 0);
    assert.equal(result.failures.length, 1);
    assert.match(result.failures[0].error, new RegExp(reason));
    assert.deepEqual(effects(), []);
    const team = mount("team");
    team.current().handleTeamDeployed(channel, result);
    assert.match(errors.at(-1) ?? "", new RegExp(reason));
  });
}
for (const owner of ["persona", "team", "profile"])
  for (const reason of [
    "definition_hosted_elsewhere",
    "device_home_sync_pending",
    "device_home_sync_failed",
  ]) {
    test(`${owner}: definition deletion refuses before team/cascade mutations (${reason})`, async () => {
      setup([blocked(reason)]);
      const surface = mount(owner);
      const cached = fromRawPersona(rawPersona());
      if (owner === "persona")
        await act(async () => surface.current().handleDelete(cached));
      if (owner === "team")
        await act(async () =>
          surface.current().handleDeleteTeam({
            id: "team",
            name: "Team",
            sourceDir: "/fixture/team",
            catalogSource: null,
            personaIds: ["persona"],
          }),
        );
      if (owner === "profile") {
        await waitFor(() => assert.ok(dialogProps));
        await act(async () => dialogProps.onConfirmDelete(cached));
        await waitFor(() => assert.ok(toastErrors.length));
      }
      assert.deepEqual(effects(), []);
      const actualReason =
        owner === "persona"
          ? surface.current().personaErrorMessage
          : owner === "team"
            ? errors.at(-1)
            : toastErrors.at(-1);
      assert.match(
        actualReason ?? "",
        new RegExp(personas[0].capabilities.blockedReason),
      );
    });
  }
test("removed team definitions report refused/skipped ids and leave remote metadata editable", async () => {
  setup();
  const surface = mount("team");
  await act(async () =>
    surface.current().handleDeleteRemovedPersonas(["persona"]),
  );
  assert.deepEqual(effects(), []);
  assert.match(errors.at(-1) ?? "", /persona.*definition_hosted_elsewhere/);
  cleanup();
  const library = mount("persona");
  await act(async () =>
    library.current().handleSubmit({ ...creationInput, id: "persona" }),
  );
  assert.ok(commands.some(([name]) => name === "update_persona"));
});
test("production builder caller inventory is exact and every owner is exercised", () => {
  const root = fileURLToPath(new URL("../../../", import.meta.url));
  const found = [];
  function visit(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) visit(full);
      else if (/\.tsx?$/.test(entry.name) && !entry.name.includes("test")) {
        const source = ts.createSourceFile(
          full,
          fs.readFileSync(full, "utf8"),
          ts.ScriptTarget.Latest,
          true,
        );
        const names = new Set(["buildInstanceInputForDefinition"]),
          namespaces = new Set();
        for (const node of source.statements)
          if (
            ts.isImportDeclaration(node) &&
            node.moduleSpecifier.text.endsWith("instanceInputForDefinition")
          ) {
            const bindings = node.importClause?.namedBindings;
            if (bindings && ts.isNamespaceImport(bindings))
              namespaces.add(bindings.name.text);
            if (bindings && ts.isNamedImports(bindings))
              for (const item of bindings.elements)
                if (
                  (item.propertyName ?? item.name).text ===
                  "buildInstanceInputForDefinition"
                )
                  names.add(item.name.text);
          }
        function scan(node) {
          if (
            ts.isCallExpression(node) &&
            (names.has(node.expression.getText(source)) ||
              (ts.isPropertyAccessExpression(node.expression) &&
                namespaces.has(node.expression.expression.getText(source)) &&
                node.expression.name.text ===
                  "buildInstanceInputForDefinition"))
          )
            found.push(path.relative(root, full));
          ts.forEachChild(node, scan);
        }
        scan(source);
      }
    }
  }
  visit(root);
  assert.deepEqual(
    found.sort(),
    [
      "features/agents/ui/useManagedAgentActions.ts",
      "features/agents/ui/usePersonaActions.ts",
      "features/agents/useAgentManagement.ts",
      "features/onboarding/welcomeGuide.ts",
      "features/profile/ui/UserProfilePanel.tsx",
    ].sort(),
  );
});
for (const owner of ["managed", "profile"]) {
  test(`${owner}: explicit shared backend permission retains create behavior under unavailable home`, async () => {
    setup([rawPersona({ share_across_devices: true, home: null })]);
    const surface = mount(owner);
    if (owner === "managed")
      await act(async () =>
        surface.current().handleStartPersona(fromRawPersona(rawPersona())),
      );
    else {
      await waitFor(() =>
        assert.equal(typeof summaryProps?.handleInstantiateAgent, "function"),
      );
      await act(async () => summaryProps.handleInstantiateAgent());
    }
    const creates = commands.filter(
      ([name]) => name === "create_managed_agent",
    );
    assert.equal(creates.length, 1);
    assert.equal(creates[0][1].input.personaId, "persona");
    assert.equal(
      owner === "managed"
        ? surface.current().actionErrorMessage
        : toastErrors.at(-1),
      owner === "managed" ? null : undefined,
    );
  });
}
test("Welcome shared definitions retain provisioning and access membership behavior", async () => {
  setup(
    welcomeGuide.WELCOME_TEAM_STARTERS.map((starter) =>
      rawPersona({
        id: starter.personaId,
        is_builtin: true,
        share_across_devices: true,
        home: null,
      }),
    ),
  );
  handlers.set("create_managed_agent", ({ input }) => {
    const agent = rawAgent({
      pubkey: String(agents.length + 1).repeat(64),
      name: input.name,
      persona_id: input.personaId,
      team_id: input.teamId,
    });
    agents.push(agent);
    return { agent, spawn_error: null, profile_sync_error: null };
  });
  await welcomeGuide.ensureWelcomeTeam("welcome", "wss://relay.example");
  assert.equal(
    commands.filter(([name]) => name === "create_managed_agent").length,
    3,
  );
  assert.equal(
    commands.filter(([name]) => name === "add_channel_members").length,
    1,
  );
  const access = commands
    .filter(([name]) => name === "update_managed_agent")
    .map(([, args]) => args.input);
  assert.equal(access.length, 2);
  for (const input of access) {
    assert.equal(input.respondTo, "allowlist");
    assert.deepEqual(input.respondToAllowlist, ["1".repeat(64)]);
  }
});
test("automatic channel batch reports each skipped member and still creates permitted shared member", async () => {
  setup([
    blocked("device_home_sync_pending"),
    rawPersona({ id: "shared", home: null, share_across_devices: true }),
  ]);
  handlers.set("create_managed_agent", ({ input }) => {
    const agent = rawAgent({ persona_id: input.personaId });
    agents.push(agent);
    return { agent, spawn_error: null, profile_sync_error: null };
  });
  const result = await channelAgents.createChannelManagedAgents("channel", [
    { runtime, name: "Blocked", personaId: "persona" },
    { runtime, name: "Shared", personaId: "shared" },
  ]);
  assert.equal(result.successes.length, 1);
  assert.equal(result.failures.length, 1);
  assert.equal(result.failures[0].personaId, "persona");
  assert.match(result.failures[0].error, /device_home_sync_pending.*retry/i);
  assert.equal(
    commands.filter(([name]) => name === "create_managed_agent").length,
    1,
  );
  assert.equal(
    commands.filter(([name]) => name === "add_channel_members").length,
    1,
  );
});
test("Welcome refuses copied existing instance before runtime or access repair and membership", async () => {
  const rows = welcomeGuide.WELCOME_TEAM_STARTERS.map((starter) =>
    rawPersona({ id: starter.personaId, is_builtin: true }),
  );
  setup(rows, [
    rawAgent({
      persona_id: rows[0].id,
      team_id: welcomeGuide.WELCOME_TEAM_ID,
      can_start_on_device: false,
    }),
  ]);
  await assert.rejects(
    () => welcomeGuide.ensureWelcomeTeam("welcome", "wss://relay.example"),
    /instance_not_runnable_on_device/,
  );
  assert.deepEqual(effects(), []);
});
test("built-in profile removal checks delete capability before cascade and deactivation", async () => {
  setup([blocked("device_home_sync_pending", { is_builtin: true })]);
  mount("profile", rawPersona({ is_builtin: true }));
  await waitFor(() =>
    assert.equal(typeof summaryProps?.onDeleteAgent, "function"),
  );
  await act(async () => summaryProps.onDeleteAgent());
  assert.match(toastErrors.at(-1) ?? "", /device_home_sync_pending.*retry/i);
  assert.deepEqual(effects(), []);
});
test("Welcome builder re-reads its exact definition before producing instance input", async () => {
  const starter = welcomeGuide.WELCOME_TEAM_STARTERS[0];
  setup([blocked("definition_hosted_elsewhere", { id: starter.personaId })]);
  await assert.rejects(
    () =>
      welcomeGuide.buildWelcomeStarterCreateInput(
        starter,
        fromRawPersona(rawPersona({ id: starter.personaId })),
        [runtime],
        "goose",
        "wss://relay.example",
      ),
    /definition_hosted_elsewhere/,
  );
  assert.deepEqual(effects(), []);
  assert.equal(commands.filter(([name]) => name === "list_personas").length, 1);
});

test("manual team deletion removes its container while leaving a remote private member intact", async () => {
  setup([blocked()]);
  const surface = mount("team");
  await act(async () =>
    surface.current().handleDeleteTeam({
      id: "manual-team",
      name: "Manual Team",
      description: null,
      instructions: null,
      personaIds: ["persona"],
      isBuiltin: false,
      shared: false,
      sourceDir: null,
      catalogSource: null,
      isSymlink: false,
      symlinkTarget: null,
      version: null,
      createdAt: "now",
      updatedAt: "now",
    }),
  );
  assert.deepEqual(effects(), [["delete_team", { id: "manual-team" }]]);
  assert.equal(errors.at(-1), null);
  assert.equal(notices.at(-1), 'Deleted team "Manual Team".');
});

test("catalog team deletion retains member capability guards before any cascade", async () => {
  setup([blocked()]);
  const surface = mount("team");
  await act(async () =>
    surface.current().handleDeleteTeam({
      id: "catalog-team",
      name: "Catalog Team",
      personaIds: ["persona"],
      sourceDir: null,
      catalogSource: { ownerPubkey: OWNER, teamDTag: "catalog-team" },
    }),
  );
  assert.deepEqual(effects(), []);
  assert.match(errors.at(-1), /definition_hosted_elsewhere/);
});
