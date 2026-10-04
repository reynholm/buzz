import assert from "node:assert/strict";
import { afterEach, before, test } from "node:test";
import React from "react";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { UnifiedAgentsSection } from "./UnifiedAgentsSection.tsx";
import { AgentDialog } from "./AgentDialog.tsx";
import {
  duplicatePersonaDialogState,
  editPersonaDialogState,
} from "./personaDialogState.ts";
import { relayClient } from "@/shared/api/relayClient";
import { personasQueryKey, usePersonasQuery } from "../hooks.ts";

// Native/network boundaries only; the mounted dialogs, fields, menus, query
// observers and IPC wrappers run unchanged. Guard removal must change the DOM
// or the payload delivered through the real form submit handler.
const PK = "a".repeat(64),
  OTHER = "b".repeat(64);
const runtime = {
  id: "goose",
  label: "Goose",
  command: "goose",
  availability: "available",
  defaultArgs: [],
  mcpCommand: "",
  avatarUrl: null,
};
const persona = {
  id: "remote",
  displayName: "Same Name",
  description: "Authored description",
  systemPrompt: "prompt",
  acpCommand: "buzz-acp",
  runtime: "goose",
  avatarUrl: "https://example.com/avatar.png",
  isActive: true,
  isBuiltIn: false,
  shared: false,
  shareAcrossDevices: false,
  home: { kind: "remote", label: "Laptop A", remoteInstancePubkeys: [PK] },
  capabilities: {
    canCreateInstance: false,
    canDeleteDefinition: false,
    blockedReason: "definition_hosted_elsewhere",
  },
};
let presence = {},
  clients = [],
  calls = [],
  presenceFailure = false;
before(() => {
  window.matchMedia = () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
  });
  window.Image = class extends window.EventTarget {
    complete = true;
    naturalWidth = 1;
    src = "";
  };
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  relayClient.getConnectionState = () => "connected";
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      calls.push([command, args]);
      switch (command) {
        case "get_presence":
          if (presenceFailure) throw new Error("presence unavailable");
          return presence;
        case "list_personas":
          return [];
        case "get_global_agent_config":
          return {
            preferred_runtime: "goose",
            provider: "ollama",
            model: "test-model",
            env_vars: {},
          };
        case "get_runtime_file_config":
          return {};
        case "get_baked_build_env_keys":
          return [];
        case "discover_acp_commands":
          return [];
        case "get_baked_build_env":
          return [];
        case "agent_access_owner_only":
          return false;
        case "discover_acp_providers":
          return [];
        case "get_identity":
          return { pubkey: OTHER };
        case "list_archived_identities":
          return { archived: [] };
        default:
          return new Promise(() => {});
      }
    },
  };
});
afterEach(() => {
  cleanup();
  for (const client of clients) client.clear();
  clients = [];
  calls = [];
  presence = {};
  presenceFailure = false;
  relayClient.getConnectionState = () => "connected";
});
function mount(element, seed = []) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  client.setQueryData(personasQueryKey, seed);
  clients.push(client);
  return render(React.createElement(QueryClientProvider, { client }, element));
}
const noop = () => {};
function CardSurface({ value, agents }) {
  usePersonasQuery();
  return section(value, agents);
}
function card(value = persona, agents = []) {
  return React.createElement(CardSurface, { value, agents });
}
function section(value, agents) {
  return React.createElement(UnifiedAgentsSection, {
    personas: value ? [value] : [],
    agents,
    defaultModel: "",
    getAvailability: () => "offline",
    startingPersonaIds: new Set(),
    isAgentsLoading: false,
    isPersonasLoading: false,
    isPersonasPending: false,
    isActionPending: false,
    onOpenAgentProfile: noop,
    onOpenPersonaProfile: noop,
    onStartPersona: noop,
    onStartAgent: noop,
    onRestartAgent: noop,
    onOpenCatalog: noop,
    onDuplicatePersona: noop,
    onEditPersona: noop,
    onSharePersona: noop,
    onDeactivatePersona: noop,
    onDeletePersona: noop,
  });
}
function dialog(initialValues, onSubmitDefinition, mode = "definition") {
  const props =
    mode === "definition"
      ? {
          definitionError: null,
          isDefinitionPending: false,
          onSubmitDefinition,
        }
      : {
          open: true,
          title: "Edit",
          description: "",
          submitLabel: "Save",
          error: null,
          isPending: false,
          onSubmit: onSubmitDefinition,
        };
  return React.createElement(AgentDialog, {
    ...props,
    mode,
    embedded: true,
    initialValues,
    runtimes: [runtime],
    runtimeCatalogStatus: "ready",
    onOpenChange: noop,
  });
}

test("create_toggle_defaults_off_and_survives_submit", async () => {
  const submitted = [];
  mount(
    dialog(
      { displayName: "New", systemPrompt: "", runtime: "goose" },
      async (input) => {
        submitted.push(input);
        return false;
      },
    ),
  );
  const checkbox = await screen.findByRole("checkbox", {
    name: "Разрешить запуск на других моих устройствах",
  });
  assert.equal(checkbox.checked, false);
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Create agent" }).disabled,
      false,
    ),
  );
  fireEvent.submit(document.getElementById("persona-dialog-form"));
  await waitFor(() => assert.equal(submitted.length, 1));
  assert.equal(submitted[0].shareAcrossDevices, false);
  fireEvent.click(checkbox);
  fireEvent.submit(document.getElementById("persona-dialog-form"));
  await waitFor(() => assert.equal(submitted.length, 2));
  assert.equal(submitted[1].shareAcrossDevices, true);
  assert.ok(
    screen.getByText(
      "При включении на другой машине можно создать отдельный экземпляр этого агента",
    ),
  );
});

test("duplicate_and_draft_default_off", async () => {
  for (const initial of [
    duplicatePersonaDialogState({ ...persona, shareAcrossDevices: true })
      .initialValues,
    { displayName: "Owner draft", systemPrompt: "", runtime: "goose" },
  ]) {
    const submitted = [];
    mount(
      dialog(initial, async (input) => {
        submitted.push(input);
        return false;
      }),
    );
    assert.equal(
      (
        await screen.findByRole("checkbox", {
          name: "Разрешить запуск на других моих устройствах",
        })
      ).checked,
      false,
    );
    await waitFor(() =>
      assert.equal(
        screen.getByRole("button", { name: "Create agent" }).disabled,
        false,
      ),
    );
    fireEvent.submit(document.getElementById("persona-dialog-form"));
    await waitFor(() => assert.equal(submitted.length, 1));
    assert.equal(submitted[0].shareAcrossDevices, false);
    cleanup();
  }
});

test("remote_card_has_no_start_or_delete", async () => {
  mount(card());
  assert.ok(
    !screen.queryByTestId("persona-runtime-start-remote"),
    "remote Start must be absent",
  );
  assert.ok(screen.getByTestId("persona-runtime-remote-remote"));
  assert.ok(screen.getByText("Same Name"));
  assert.ok(screen.getByText("Authored description"));
  await waitFor(() =>
    assert.ok(
      document.querySelector('img[src="https://example.com/avatar.png"]'),
    ),
  );
  fireEvent.pointerDown(
    screen.getByRole("button", { name: "Open actions for Same Name" }),
    { button: 0, ctrlKey: false },
  );
  assert.ok(await screen.findByRole("menuitem", { name: "Edit" }));
  assert.ok(
    !screen.queryByRole("menuitem", { name: "Delete" }),
    "remote Delete must be absent",
  );
});

test("presence_uses_pubkey_not_name", async () => {
  presence = { [OTHER]: "online" };
  const view = mount(card());
  assert.ok(await screen.findByText("На устройстве Laptop A · не в сети"));
  assert.deepEqual(calls.find(([cmd]) => cmd === "get_presence")[1].pubkeys, [
    PK,
  ]);
  presence = { [PK]: "away", [OTHER]: "offline" };
  for (const client of clients)
    await act(() => client.invalidateQueries({ queryKey: ["presence"] }));
  assert.ok(await screen.findByText("На устройстве Laptop A · в сети"));
  view.rerender(
    React.createElement(
      QueryClientProvider,
      { client: clients[0] },
      card({ ...persona, home: { ...persona.home, label: null } }),
    ),
  );
  assert.ok(await screen.findByText(/На другом устройстве/));
});

test("pending_failed_and_missing_projection_hide_actions_but_allow_sync_retry", async () => {
  for (const reason of [
    "device_home_sync_pending",
    "device_home_sync_failed",
    undefined,
  ]) {
    mount(
      card({
        ...persona,
        home: reason
          ? { kind: "unclaimed", label: null, remoteInstancePubkeys: [] }
          : undefined,
        capabilities: reason
          ? {
              canCreateInstance: false,
              canDeleteDefinition: false,
              blockedReason: reason,
            }
          : undefined,
      }),
    );
    assert.ok(
      !screen.queryByRole("button", { name: "Start Agent" }),
      "unauthorized Start must be absent",
    );
    if (reason === "device_home_sync_pending")
      assert.ok(screen.getByText("Проверяем размещение агента"));
    else {
      assert.ok(screen.getByRole("button", { name: "Обновить состояние" }));
      fireEvent.click(
        screen.getByRole("button", { name: "Обновить состояние" }),
      );
      await waitFor(() =>
        assert.ok(calls.some(([cmd]) => cmd === "list_personas")),
      );
    }
    cleanup();
    calls = [];
  }
});

test("explicit_backend_permission_with_unknown_home_retains_start_and_delete", async () => {
  mount(
    card({
      ...persona,
      shareAcrossDevices: true,
      home: null,
      capabilities: {
        canCreateInstance: true,
        canDeleteDefinition: true,
        blockedReason: null,
      },
    }),
  );
  assert.ok(screen.getByTestId("persona-runtime-start-remote"));
  fireEvent.pointerDown(
    screen.getByRole("button", { name: "Open actions for Same Name" }),
    { button: 0, ctrlKey: false },
  );
  assert.ok(await screen.findByRole("menuitem", { name: "Delete" }));
});

test("copied_local_instance_does_not_restore_start", () => {
  mount(
    card(
      {
        ...persona,
        home: { kind: "local", label: "Here", remoteInstancePubkeys: [] },
        capabilities: {
          canCreateInstance: true,
          canDeleteDefinition: true,
          blockedReason: null,
        },
      },
      [
        {
          pubkey: PK,
          name: "Same Name",
          personaId: persona.id,
          status: "stopped",
          backend: { type: "local" },
          canStartOnDevice: false,
        },
      ],
    ),
  );
  assert.ok(
    !screen.queryByRole("button", { name: "Start Agent" }),
    "unauthorized Start must be absent",
  );
  assert.ok(screen.getByTestId("persona-runtime-remote-remote"));
});

test("edit_policy_is_readonly_and_omitted_from_update", async () => {
  const submitted = [];
  mount(
    dialog(
      editPersonaDialogState(persona).initialValues,
      async (input) => {
        submitted.push(input);
      },
      "definition-edit",
    ),
    [persona],
  );
  assert.ok(
    !screen.queryByRole("checkbox", {
      name: "Разрешить запуск на других моих устройствах",
    }),
    "edit must not expose policy toggle",
  );
  assert.ok(await screen.findByText("Работает на: Laptop A"));
  assert.ok(
    screen.getByText(/Разрешить запуск на других моих устройствах: выключено/),
  );
  await waitFor(() =>
    assert.equal(screen.getByRole("button", { name: "Save" }).disabled, false),
  );
  fireEvent.submit(document.getElementById("persona-dialog-form"));
  await waitFor(() => assert.equal(submitted.length, 1));
  assert.equal("shareAcrossDevices" in submitted[0], false);
});

test("failed_or_disconnected_presence_is_unknown_without_restoring_actions", async () => {
  for (const failed of [true, false]) {
    presenceFailure = failed;
    if (!failed) relayClient.getConnectionState = () => "disconnected";
    mount(card());
    assert.ok(
      await screen.findByText("На устройстве Laptop A · связь неизвестна"),
    );
    assert.ok(!screen.queryByRole("button", { name: "Start Agent" }));
    cleanup();
  }
});

test("standalone_instance_requires_exact_backend_start_permission", () => {
  mount(
    card(null, [
      {
        pubkey: PK,
        name: "Standalone",
        status: "stopped",
        backend: { type: "local" },
        canStartOnDevice: false,
      },
    ]),
  );
  assert.ok(
    !screen.queryByRole("button", { name: "Start Agent" }),
    "standalone instance must not ignore backend start refusal",
  );
});
