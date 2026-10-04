import { expect, test } from "@playwright/test";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { waitForAnimations } from "../helpers/animations";

const REMOTE = TEST_IDENTITIES.charlie.pubkey;
const LOCAL = "d".repeat(64);
const OWNER = "deadbeef".repeat(8);
const PERSONA = "shared-persona";

test("device home IPC supplies explicit authority for a local definition", async ({
  page,
}) => {
  await installMockBridge(page, {
    personas: [
      { id: PERSONA, displayName: "Local definition", systemPrompt: "Local." },
    ],
  });
  await page.goto("/");
  await expect(page.getByTestId("open-agents-view")).toBeVisible();
  const projection = await page.evaluate(async (id) => {
    const invoke = window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
    if (!invoke) throw new Error("E2E bridge has not initialized");
    const identity = await invoke("get_device_identity", {});
    const session = (await invoke("begin_device_home_sync", {})) as {
      token: string;
    };
    const history = await invoke("hydrate_device_home_history", {
      sessionToken: session.token,
    });
    await invoke("finish_device_home_sync", { sessionToken: session.token });
    const personas = (await invoke("list_personas", {})) as Array<{
      id: string;
    }>;
    return { identity, history, persona: personas.find((p) => p.id === id) };
  }, PERSONA);
  expect(projection.identity).toMatchObject({ label: "Mock desktop" });
  expect(projection.history).toMatchObject({
    coveredEventIds: expect.any(Array),
  });
  expect(projection.persona).toMatchObject({
    share_across_devices: false,
    home: { kind: "local", label: "Mock desktop" },
    capabilities: {
      canCreateInstance: true,
      canDeleteDefinition: true,
      blockedReason: null,
    },
  });
  await page.getByTestId("open-agents-view").click();
  await page.getByTestId(`persona-agent-row-${PERSONA}`).click();
  await expect(page.getByTestId("user-profile-start-agent")).toBeVisible();
});

for (const state of ["remote", "pending", "failed", "unresolved"] as const) {
  test(`definition on ${state} device offers no spawn or delete authority`, async ({
    page,
  }) => {
    const reason =
      state === "remote"
        ? "definition_hosted_elsewhere"
        : state === "pending"
          ? "device_home_sync_pending"
          : "device_home_sync_failed";
    await installMockBridge(page, {
      personas: [
        {
          ...(state === "remote" ? {} : { id: PERSONA }),
          displayName: "Device-bound agent",
          systemPrompt: "Still editable.",
          home:
            state === "unresolved"
              ? null
              : {
                  kind: state === "remote" ? "remote" : "unclaimed",
                  label: state === "remote" ? "Other laptop" : null,
                  remoteInstancePubkeys: state === "remote" ? [REMOTE] : [],
                },
          capabilities: {
            canCreateInstance: false,
            canDeleteDefinition: false,
            blockedReason: reason,
          },
        },
      ],
    });
    await page.goto("/#/agents");
    const row = page.locator('[data-testid^="persona-agent-row-"]').filter({
      has: page.getByRole("button", {
        name: "Device-bound agent agent profile",
      }),
    });
    await expect(row).toContainText(
      state === "remote"
        ? "On device Other laptop"
        : state === "pending"
          ? "Checking where this agent runs"
          : "Could not verify where this agent runs",
    );
    await row
      .getByRole("button", { name: "Device-bound agent agent profile" })
      .press("Enter");
    const panel = page.getByTestId("user-profile-panel");
    await expect(panel).toBeVisible();
    await expect(page.getByTestId("user-profile-start-agent")).toHaveCount(0);
    await expect(page.getByTestId("user-profile-edit-agent")).toBeVisible();
    await page.getByTestId("user-profile-tab-runtime").click();
    await expect(page.getByTestId("user-profile-delete-agent-row")).toHaveCount(
      0,
    );
    const commands = await page.evaluate(
      () => window.__BUZZ_E2E_COMMANDS__ ?? [],
    );
    expect(commands).not.toContain("create_managed_agent");
    expect(commands).not.toContain("start_managed_agent");
    expect(commands).not.toContain("delete_persona");
  });
}

test("denied instance retains Stop but offers no Restart despite config drift", async ({
  page,
}) => {
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: LOCAL,
        name: "Copied instance",
        personaId: PERSONA,
        status: "running",
        canStartOnDevice: false,
        needsRestart: true,
      },
    ],
    personas: [
      {
        id: PERSONA,
        displayName: "Device-bound agent",
        systemPrompt: "Remote.",
        home: {
          kind: "remote",
          label: "Other laptop",
          remoteInstancePubkeys: [LOCAL],
        },
        capabilities: {
          canCreateInstance: false,
          canDeleteDefinition: false,
          blockedReason: "definition_hosted_elsewhere",
        },
      },
    ],
  });
  await page.goto(`/#/agents?profile=${LOCAL}`);
  const panel = page.getByTestId("user-profile-panel");
  await expect(panel).toBeVisible();
  await expect(
    page.getByTestId("user-profile-agent-primary-action"),
  ).toHaveAttribute("aria-label", "Stop");
  await expect(panel.getByRole("button", { name: /restart/i })).toHaveCount(0);
  await page.getByTestId("user-profile-agent-primary-action").click();
  await expect(
    page.getByTestId("user-profile-agent-primary-action"),
  ).toHaveCount(0);
  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).toContain("stop_managed_agent");
  expect(commands).not.toContain("start_managed_agent");
});

for (const hasSibling of [false, true]) {
  test(`explicit relay-only identity has no local controls (${hasSibling ? "local sibling" : "persona only"})`, async ({
    page,
  }, testInfo) => {
    await installMockBridge(page, {
      oaOwnerIsMe: true,
      managedAgents: hasSibling
        ? [
            {
              pubkey: LOCAL,
              name: "Local sibling B",
              personaId: PERSONA,
              status: "running",
              channelNames: ["agents"],
            },
          ]
        : [],
      personas: [
        {
          id: PERSONA,
          displayName: "Shared persona P",
          isActive: true,
          systemPrompt: "Local definition, not the remote identity.",
        },
      ],
      searchProfiles: [
        {
          pubkey: REMOTE,
          displayName: "Relay agent A",
          ownerPubkey: OWNER,
          isAgent: true,
        },
      ],
    });
    await page.goto(`/#/agents?profile=${REMOTE}`);
    const panel = page.getByTestId("user-profile-panel");
    await expect(panel).toBeVisible();
    await expect(page.getByTestId("user-profile-name-row")).toContainText(
      "Relay agent A",
    );
    for (const testId of [
      "user-profile-agent-primary-action",
      "user-profile-start-agent",
      "user-profile-edit-agent",
      "user-profile-add-to-channel",
    ]) {
      await expect(page.getByTestId(testId)).toHaveCount(0);
    }
    await expect(panel).not.toContainText(
      "Local definition, not the remote identity.",
    );
    await waitForAnimations(page);
    await panel.screenshot({
      path: testInfo.outputPath("exact-relay-identity.png"),
    });

    // Persona-only navigation remains legitimate and intentionally different.
    await page.getByTestId("auxiliary-panel-close").click();
    await page.getByTestId(`persona-agent-row-${PERSONA}`).click();
    await expect(
      page.getByTestId(
        hasSibling
          ? "user-profile-agent-primary-action"
          : "user-profile-start-agent",
      ),
    ).toBeVisible();
    await waitForAnimations(page);
    await panel.screenshot({
      path: testInfo.outputPath("explicit-persona.png"),
    });
  });
}

for (const allArchived of [false, true]) {
  test(`archived exact key stays navigable (${allArchived ? "all archived" : "live sibling"})`, async ({
    page,
  }) => {
    await installMockBridge(page, {
      oaOwnerIsMe: true,
      archivedIdentities: allArchived ? [REMOTE, LOCAL] : [REMOTE],
      managedAgents: [
        {
          pubkey: REMOTE,
          name: "Archived A",
          personaId: PERSONA,
          status: "stopped",
          channelNames: ["agents"],
        },
        {
          pubkey: LOCAL,
          name: "Sibling B",
          personaId: PERSONA,
          status: "running",
          channelNames: ["agents"],
        },
      ],
      personas: [
        {
          id: PERSONA,
          displayName: "Shared persona P",
          isActive: true,
          systemPrompt: "Archive profile fixture.",
        },
      ],
    });
    await page.goto(`/#/agents?profile=${REMOTE}`);
    await expect(page.getByTestId("user-profile-panel")).toBeVisible();
    await expect(page.getByTestId("user-profile-archived-flair")).toBeVisible();
    await expect(
      page.getByTestId("user-profile-agent-primary-action"),
    ).toHaveAttribute("aria-label", "Start agent");
    await expect(page.getByTestId("user-profile-name-row")).toContainText(
      "Archived A",
    );
    // Persona navigation still excludes archived representatives.
    await page.getByTestId("auxiliary-panel-close").click();
    await page.getByTestId(`persona-agent-row-${PERSONA}`).click();
    if (allArchived) {
      await expect(page.getByTestId("user-profile-start-agent")).toBeVisible();
      await expect(
        page.getByTestId("user-profile-agent-primary-action"),
      ).toHaveCount(0);
    } else {
      await expect(
        page.getByTestId("user-profile-agent-primary-action"),
      ).toHaveAttribute("aria-label", "Stop");
    }
  });
}
