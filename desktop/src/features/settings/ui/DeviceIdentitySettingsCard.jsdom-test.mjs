import assert from "node:assert/strict";
import { registerHooks } from "node:module";
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

// Other settings own unrelated native providers; keep the real Agents panel
// and device card, with only those other cards removed from this mount.
registerHooks({
  load(url, context, nextLoad) {
    for (const name of [
      "AgentDefaultsSettingsCard",
      "HarnessesSettingsPanel",
      "PreventSleepSettingsCard",
    ])
      if (url.endsWith(`/${name}.tsx`))
        return {
          format: "module",
          shortCircuit: true,
          source: `export const ${name} = () => null;`,
        };
    return nextLoad(url, context);
  },
});
const { AgentsSettingsPanel } = await import("./AgentsSettingsPanel.tsx");
const rawIdentity = {
  device_id: "device",
  label: "Laptop A",
  created_at: "now",
};
let publication = "queued",
  failure = null,
  readFailure = null,
  commands = [],
  clients = [];
before(() => {
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      commands.push([cmd, args]);
      if (cmd === "get_device_identity") {
        if (readFailure) throw new Error(readFailure);
        return rawIdentity;
      }
      if (cmd === "set_device_label") {
        if (failure) throw new Error(failure);
        return { identity: { ...rawIdentity, label: args.label }, publication };
      }
      return new Promise(() => {});
    },
  };
});
afterEach(() => {
  cleanup();
  for (const client of clients) client.clear();
  clients = [];
  commands = [];
  publication = "queued";
  failure = null;
  readFailure = null;
});
function mount() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  clients.push(client);
  return render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(AgentsSettingsPanel),
    ),
  );
}

test("label_keyboard_save_reports_queued", async () => {
  mount();
  const input = await screen.findByRole("textbox", {
    name: "Имя этого устройства",
  });
  await waitFor(() => assert.equal(input.value, "Laptop A"));
  fireEvent.change(input, { target: { value: "  Laptop B  " } });
  input.focus();
  // Native form submission is Enter's browser action; hit the real form path.
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.submit(input.closest("form"));
  assert.ok(await screen.findByRole("status"));
  await waitFor(() =>
    assert.match(screen.getByRole("status").textContent, /очереди/),
  );
  assert.deepEqual(commands.find(([cmd]) => cmd === "set_device_label")[1], {
    label: "Laptop B",
  });
  assert.doesNotMatch(screen.getByRole("status").textContent, /завершено/);
});

test("failed_label_save_is_visible_and_retryable", async () => {
  mount();
  const input = await screen.findByRole("textbox", {
    name: "Имя этого устройства",
  });
  await waitFor(() => assert.equal(input.value, "Laptop A"));
  failure = "durable enqueue failed";
  fireEvent.change(input, { target: { value: "Laptop B" } });
  fireEvent.click(screen.getByRole("button", { name: "Сохранить" }));
  assert.match(
    (await screen.findByRole("alert")).textContent,
    /durable enqueue failed/,
  );
  assert.equal(input.value, "Laptop B");
  failure = null;
  publication = "complete";
  fireEvent.submit(input.closest("form"));
  await waitFor(() =>
    assert.match(screen.getByRole("status").textContent, /завершено/),
  );
});

test("empty_or_whitespace_label_never_invokes_write", async () => {
  mount();
  const input = await screen.findByRole("textbox", {
    name: "Имя этого устройства",
  });
  await waitFor(() => assert.equal(input.value, "Laptop A"));
  for (const value of ["", "   "]) {
    fireEvent.change(input, { target: { value } });
    assert.equal(
      screen.getByRole("button", { name: "Сохранить" }).disabled,
      true,
    );
    await act(async () => fireEvent.submit(input.closest("form")));
  }
  assert.equal(
    commands.filter(([cmd]) => cmd === "set_device_label").length,
    0,
  );
});

test("identity_read_error_retains_retry_affordance", async () => {
  readFailure = "keychain unavailable";
  mount();
  assert.match(
    (await screen.findByRole("alert")).textContent,
    /keychain unavailable/,
  );
  readFailure = null;
  fireEvent.click(screen.getByRole("button", { name: "Повторить" }));
  const input = await screen.findByRole("textbox", {
    name: "Имя этого устройства",
  });
  await waitFor(() => assert.equal(input.value, "Laptop A"));
});
