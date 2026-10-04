import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { afterEach, test } from "node:test";
import React from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";

registerHooks({
  load(url, context, nextLoad) {
    let source;
    if (url.endsWith("/SettingsPanels.tsx"))
      source =
        'export const settingsSections = []; export const DEFAULT_SETTINGS_SECTION = "appearance"; export const renderSettingsSection = () => null;';
    if (url.endsWith("/shared/features/index.ts"))
      source =
        "export const useFeatureSnapshot = () => null; export const getFeature = () => null; export const resolveEnabled = () => true;";
    if (url.endsWith("/features/community-members/hooks.ts"))
      source =
        "export const useMyRelayMembershipLookupQuery = () => ({ data: null });";
    if (url.endsWith("/shared/ui/sidebar.tsx"))
      source = `import React from "react";
        const Container = ({ children, "data-testid": testid }) => React.createElement("div", { "data-testid": testid }, children);
        export const Sidebar = Container, SidebarContent = Container, SidebarFooter = Container, SidebarGroup = Container, SidebarGroupContent = Container, SidebarGroupLabel = Container, SidebarHeader = Container, SidebarInset = Container, SidebarMenu = Container, SidebarMenuButton = Container, SidebarMenuItem = Container;
        export const useSidebar = () => ({ isMobile: false, open: true, setOpen: () => {} });`;
    if (source) return { format: "module", shortCircuit: true, source };
    return nextLoad(url, context);
  },
});
const { SettingsView } = await import("./SettingsView.tsx");
const identity = {
  fork_revision: "1",
  commit_sha: "a".repeat(40),
  base_tag: "desktop-v0.5.26",
};
let commands = [];
afterEach(() => {
  cleanup();
  commands = [];
});
function mount(raw) {
  window.requestAnimationFrame = (fn) => setTimeout(fn, 0);
  window.cancelAnimationFrame = clearTimeout;
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd) => {
      commands.push(cmd);
      if (cmd === "plugin:app|version") return "0.5.26";
      if (cmd === "get_fork_build_identity") return raw;
      return null;
    },
  };
  render(
    React.createElement(SettingsView, {
      section: "appearance",
      onClose() {},
      onSectionChange() {},
    }),
  );
}
test("About renders native fork identity beside unchanged upstream version", async () => {
  mount(identity);
  await waitFor(() =>
    assert.equal(screen.getByTestId("settings-version").textContent, "v0.5.26"),
  );
  await waitFor(() =>
    assert.equal(
      screen.getByTestId("settings-fork-identity").textContent,
      `Fork 1 · ${identity.commit_sha}`,
    ),
  );
  assert.ok(commands.includes("get_fork_build_identity"));
});
test("upstream About has only upstream version when native identity is absent", async () => {
  mount(null);
  await waitFor(() =>
    assert.equal(screen.getByTestId("settings-version").textContent, "v0.5.26"),
  );
  await waitFor(() => assert.ok(commands.includes("get_fork_build_identity")));
  assert.ok(screen.queryByTestId("settings-fork-identity") === null);
});
