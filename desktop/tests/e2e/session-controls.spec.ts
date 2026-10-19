import { expect, test, type Page, type Locator } from "@playwright/test";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { waitForAnimations } from "../helpers/animations";
const AGENT = "a4".repeat(32);
const VIEWER = "deadbeef".repeat(8);
const OWNER = "b4".repeat(32);
const SECOND_AGENT = "c4".repeat(32);
const CHANNEL = "agents";
const SHOT = "test-results/session-controls";
async function setup(
  page: Page,
  revoked = false,
  denied = false,
  rootAgeSeconds = 0,
) {
  await installMockBridge(page, {
    ownerOnlyAccessBuild: true,
    managedAgents: [],
    relayAgentRevalidationRevokedPubkeys: revoked ? [AGENT] : [],
    searchProfiles: [
      {
        pubkey: AGENT,
        displayName: "ControlBot",
        ownerPubkey: OWNER,
        isAgent: true,
      },
      {
        pubkey: SECOND_AGENT,
        displayName: "SecondBot",
        ownerPubkey: OWNER,
        isAgent: true,
      },
    ],
    relayAgents: [
      {
        pubkey: AGENT,
        name: "ControlBot",
        ownerPubkey: OWNER,
        respondTo: "allowlist",
        respondToAllowlist: denied ? [] : [VIEWER],
        channelNames: [CHANNEL],
      },
      {
        pubkey: SECOND_AGENT,
        name: "SecondBot",
        ownerPubkey: OWNER,
        respondTo: "allowlist",
        respondToAllowlist: [VIEWER],
        channelNames: [CHANNEL],
      },
    ],
  });
  await page.goto("/");
  await page.getByTestId(`channel-${CHANNEL}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(CHANNEL);
  await expect
    .poll(() =>
      page.evaluate(
        (channelName) =>
          window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({ channelName }) ??
          false,
        CHANNEL,
      ),
    )
    .toBe(true);
  const event = await page.evaluate(
    ({ pubkey, channelName, rootAgeSeconds }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        pubkey,
        content: "A known agent can recover without optional telemetry.",
        createdAt: Math.floor(Date.now() / 1000) - rootAgeSeconds,
      }),
    { pubkey: AGENT, channelName: CHANNEL, rootAgeSeconds },
  );
  if (!event?.id) throw new Error("Agent event not emitted");
  await expect(row(page, event.id)).toBeVisible();
  return event.id;
}
function row(page: Page, id: string) {
  return page.locator(`[data-testid="message-row"][data-message-id="${id}"]`);
}
async function openThread(page: Page, root: string) {
  await row(page, root).hover();
  await page.getByTestId(`reply-message-${root}`).click();
  await expect(page.getByTestId("message-thread-panel")).toBeVisible();
}
async function captureMenu(page: Page, trigger: Locator, name: string) {
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOT}/${name}-full.png` });
  const boxes = await Promise.all([
    trigger.boundingBox(),
    ...(await page
      .getByRole("menu")
      .all()
      .then((menus) => menus.map((menu) => menu.boundingBox()))),
  ]);
  const visible = boxes.filter(
    (box): box is NonNullable<typeof box> => box !== null,
  );
  const viewport = page.viewportSize();
  if (!viewport || visible.length < 2)
    throw new Error("Menu capture geometry unavailable");
  const x = Math.max(
    0,
    Math.floor(Math.min(...visible.map((box) => box.x)) - 8),
  );
  const y = Math.max(
    0,
    Math.floor(Math.min(...visible.map((box) => box.y)) - 8),
  );
  const right = Math.min(
    viewport.width,
    Math.ceil(Math.max(...visible.map((box) => box.x + box.width)) + 8),
  );
  const bottom = Math.min(
    viewport.height,
    Math.ceil(Math.max(...visible.map((box) => box.y + box.height)) + 8),
  );
  await waitForAnimations(page);
  await page.screenshot({
    path: `${SHOT}/${name}-clip.png`,
    clip: { x, y, width: right - x, height: bottom - y },
  });
}
async function sends(page: Page) {
  return page.evaluate(() =>
    (window.__BUZZ_E2E_COMMAND_LOG__ ?? []).filter(
      (call) => call.command === "send_channel_message",
    ),
  );
}
async function expectDescriptionStyle(item: Locator) {
  const style = await item.evaluate((el) => {
    const id = el.getAttribute("aria-describedby");
    const description = id ? document.getElementById(id) : null;
    const title = description?.previousElementSibling;
    if (!description || !title) throw new Error("Menu description unavailable");
    const titleStyle = getComputedStyle(title);
    const descriptionStyle = getComputedStyle(description);
    return {
      titleSize: Number.parseFloat(titleStyle.fontSize),
      descriptionSize: Number.parseFloat(descriptionStyle.fontSize),
      titleColor: titleStyle.color,
      descriptionColor: descriptionStyle.color,
    };
  });
  expect(style.descriptionSize).toBeLessThan(style.titleSize);
  expect(style.descriptionColor).not.toBe(style.titleColor);
}
test("known_agent_without_telemetry_still_has_current_scope_controls", async ({
  page,
}) => {
  const root = await setup(page);
  const trigger = row(page, root).getByRole("button", {
    name: "Manage current session",
  });
  await expect(trigger).toBeVisible();
  await waitForAnimations(page);
  await row(page, root).screenshot({ path: `${SHOT}/before-controls.png` });
  await expect(
    row(page, root).getByTestId("message-session-status"),
  ).toHaveCount(0);
  const draft = page.getByTestId("message-input");
  await draft.fill("Preserve this unfinished request");
  await trigger.focus();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("menuitem", { name: "Compact context", exact: true }),
  ).toBeVisible();
  const restart = page.getByRole("menuitem", { name: "Restart", exact: true });
  const restartDescription =
    "Stops current work, including subagents. File changes are kept.";
  await expect(
    restart.getByText(restartDescription, { exact: true }),
  ).toBeVisible();
  await expect(restart).toHaveAccessibleDescription(restartDescription);
  await expectDescriptionStyle(restart);
  await captureMenu(
    page,
    row(page, root).getByTestId("manage-current-session"),
    "menu-v3",
  );
  await page.keyboard.press("Home");
  await expect(restart).toBeFocused();
  await page.keyboard.press("ArrowRight");
  for (const [label, description] of [
    ["From scratch", "Keep the original assignment and your next request."],
    ["Recent history", "Keep the original assignment and recent messages."],
    [
      "Entire thread",
      "Use the available thread history, within the history limit.",
    ],
  ]) {
    const option = page.getByRole("menuitem", { name: label, exact: true });
    await expect(option.getByText(description, { exact: true })).toBeVisible();
    await expect(option).toHaveAccessibleDescription(description);
    await expectDescriptionStyle(option);
  }
  await captureMenu(
    page,
    row(page, root).getByTestId("manage-current-session"),
    "restart-submenu-v3",
  );
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(trigger).toBeFocused();
  expect(await sends(page)).toHaveLength(0);
  await trigger.click();
  await page.getByRole("menuitem", { name: "Restart", exact: true }).hover();
  await page
    .getByRole("menuitem", {
      name: "From scratch",
      exact: true,
    })
    .click();
  await expect.poll(async () => (await sends(page)).length).toBe(1);
  const call = (await sends(page))[0].payload as Record<string, unknown>;
  expect(call.content).toBe("!restart none");
  expect(call.mentionPubkeys).toEqual([AGENT]);
  expect(call.parentEventId).toBe(root);
  expect(call.rootEventId).toBe(root);
  expect(call.expectedSignerPubkey).toBe(VIEWER);
  expect(call.expectedRelayUrl).toBeTruthy();
  await expect(draft).toHaveText("Preserve this unfinished request");
  await openThread(page, root);
  await expect(page.getByTestId("message-session-control")).toContainText(
    "Start over from scratch",
  );
  await expect(page.getByTestId("message-session-control")).toContainText(
    "Result not confirmed",
  );
  const log = await page.evaluate(() => window.__BUZZ_E2E_COMMANDS__ ?? []);
  for (const command of [
    "start_managed_agent",
    "create_managed_agent",
    "attach_managed_agent",
    "add_channel_members",
  ])
    expect(log).not.toContain(command);
  await waitForAnimations(page);
  await page
    .getByTestId("message-session-control")
    .screenshot({ path: `${SHOT}/request-row.png` });
  const commandRow = page
    .getByTestId("message-row")
    .filter({ has: page.getByTestId("message-session-control") });
  const commandId = await commandRow.getAttribute("data-message-id");
  if (!commandId) throw new Error("Missing command event identity");
  const raw = await page.evaluate(
    async (eventId) =>
      JSON.parse(
        String(
          await window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("get_event", {
            eventId,
          }),
        ),
      ),
    commandId,
  );
  expect(raw.kind).toBe(9);
  expect(raw.content).toBe("!restart none");
  expect(raw.tags.filter((tag: string[]) => tag[0] === "p")).toEqual([
    ["p", AGENT],
  ]);
  expect(raw.tags.filter((tag: string[]) => tag[0] === "e")).toEqual([
    ["e", root, "", "reply"],
  ]);
  const report = async (pubkey: string, outcome: string, content: string) =>
    page.evaluate(
      ({ pubkey, outcome, content, commandId, channelName }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          pubkey,
          kind: 7,
          content,
          extraTags: [
            ["e", commandId],
            ["buzz-session-result", "1", commandId, outcome, ""],
          ],
        }),
      { pubkey, outcome, content, commandId, channelName: CHANNEL },
    );
  await report(VIEWER, "ok", "✅");
  await expect(page.getByTestId("message-session-control")).not.toContainText(
    "Completed",
  );
  await report(AGENT, "pending", "⏳");
  await expect(page.getByTestId("message-session-control")).toContainText(
    "Waiting for current work to stop",
  );
  await report(AGENT, "ok", "✅");
  await expect(page.getByTestId("message-session-control")).toContainText(
    "Completed",
  );
  await waitForAnimations(page);
  await commandRow.screenshot({ path: `${SHOT}/confirmed-row.png` });
});
test("revoked send-time permission preserves draft and publishes nothing", async ({
  page,
}) => {
  const root = await setup(page, true);
  await page.getByTestId("message-input").fill("Keep my draft");
  await row(page, root)
    .getByRole("button", { name: "Manage current session" })
    .click();
  await page
    .getByRole("menuitem", { name: "Compact context", exact: true })
    .click();
  await expect(row(page, root).getByRole("alert")).toContainText(
    "Could not send session control",
  );
  expect(await sends(page)).toHaveLength(0);
  await expect(page.getByTestId("message-input")).toHaveText("Keep my draft");
});

for (const [label, body] of [
  ["Compact context", "!compact"],
  ["Recent history", "!restart"],
  ["Entire thread", "!restart all"],
] as const) {
  test(`${label} on a nested agent reply sends to its canonical root`, async ({
    page,
  }) => {
    const root = await setup(page);
    const reply = await page.evaluate(
      ({ root, pubkey, channelName }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          pubkey,
          parentEventId: root,
          content: "A reply in the current scope.",
        }),
      { root, pubkey: AGENT, channelName: CHANNEL },
    );
    if (!reply?.id) throw new Error("Agent reply not emitted");
    await openThread(page, root);
    await page
      .getByTestId("message-thread-panel")
      .locator(`[data-message-id="${reply.id}"]`)
      .getByRole("button", { name: "Manage current session" })
      .click();
    if (label !== "Compact context")
      await page
        .getByRole("menuitem", { name: "Restart", exact: true })
        .hover();
    await page.getByRole("menuitem", { name: label, exact: true }).click();
    await expect.poll(async () => (await sends(page)).length).toBe(1);
    const payload = (await sends(page))[0].payload as Record<string, unknown>;
    expect(payload.content).toBe(body);
    expect(payload.parentEventId).toBe(root);
    expect(payload.rootEventId).toBe(root);
    expect(payload.mentionPubkeys).toEqual([AGENT]);
  });
}

test("two agent menus retain the selected signer and sibling scope", async ({
  page,
}) => {
  const firstRoot = await setup(page);
  const second = await page.evaluate(
    ({ pubkey, channelName }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        pubkey,
        content: "Second agent in an independent thread.",
      }),
    { pubkey: SECOND_AGENT, channelName: CHANNEL },
  );
  if (!second?.id) throw new Error("Second agent event not emitted");
  await row(page, second.id)
    .getByRole("button", { name: "Manage current session" })
    .click();
  await page.getByRole("menuitem", { name: "Restart", exact: true }).hover();
  await page
    .getByRole("menuitem", { name: "Recent history", exact: true })
    .click();
  await expect.poll(async () => (await sends(page)).length).toBe(1);
  const payload = (await sends(page))[0].payload as Record<string, unknown>;
  expect(payload.content).toBe("!restart");
  expect(payload.mentionPubkeys).toEqual([SECOND_AGENT]);
  expect(payload.rootEventId).toBe(second.id);
  expect(payload.parentEventId).toBe(second.id);
  await expect(row(page, firstRoot)).toContainText(
    "A known agent can recover without optional telemetry.",
  );
  await expect(
    row(page, firstRoot).getByTestId("message-session-control"),
  ).toHaveCount(0);
});

test("denied viewer has no menu while another accessible agent retains recovery", async ({
  page,
}) => {
  const root = await setup(page, false, true);
  await expect(row(page, root)).toBeVisible();
  await expect(
    row(page, root).getByRole("button", { name: "Manage current session" }),
  ).toHaveCount(0);
  expect(await sends(page)).toHaveLength(0);
});

test("correct-clock viewer sees expired old request and validated manual actor", async ({
  page,
}) => {
  const root = await setup(page, false, false, 1202);
  // Load existing history when opening the thread. Backdated live events after
  // an empty initial fetch do not model how an expired request is encountered.
  // Generic seeded replies intentionally include their author p. Confirm that
  // such a human target remains ordinary; it is not native send output.
  const generic = await page.evaluate(
    ({ root, actor, target, channelName }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        pubkey: actor,
        content: "!compact",
        parentEventId: root,
        mentionPubkeys: [target],
        createdAt: Math.floor(Date.now() / 1000) - 1201,
      }),
    { root, actor: VIEWER, target: AGENT, channelName: CHANNEL },
  );
  if (!generic?.id) throw new Error("Generic reply not emitted");
  const old = await page.evaluate(
    ({ root, pubkey, channelName }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        pubkey,
        kind: 9,
        content: "!compact",
        mentionPubkeys: ["a4".repeat(32)],
        extraTags: [["e", root, "", "reply"]],
        createdAt: Math.floor(Date.now() / 1000) - 1200,
      }),
    { root, pubkey: VIEWER, channelName: CHANNEL },
  );
  if (!old?.id) throw new Error("Old request not emitted");
  const stored = await page.evaluate(
    async (ids) =>
      Promise.all(
        ids.map(async (eventId) =>
          JSON.parse(
            String(
              await window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("get_event", {
                eventId,
              }),
            ),
          ),
        ),
      ),
    [generic.id, old.id],
  );
  expect(stored[0].tags.filter((tag: string[]) => tag[0] === "p")).toEqual([
    ["p", VIEWER],
    ["p", AGENT],
  ]);
  expect(stored[1].tags.filter((tag: string[]) => tag[0] === "p")).toEqual([
    ["p", AGENT],
  ]);
  expect(stored[1].tags.filter((tag: string[]) => tag[0] === "e")).toEqual([
    ["e", root, "", "reply"],
  ]);
  await openThread(page, root);
  await expect(row(page, generic.id)).toContainText("!compact");
  await expect(
    row(page, generic.id).getByTestId("message-session-control"),
  ).toHaveCount(0);
  await expect(
    row(page, old.id).getByTestId("message-session-control"),
  ).toContainText("Request expired · Result not confirmed");
  const transition = "7f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";
  const response = await page.evaluate(
    ({ root, command, transition, pubkey, actor, channelName }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        pubkey,
        content: "A manually restarted session.",
        parentEventId: root,
        extraTags: [
          ["buzz-session", "restarted", "user_restart:recent", ""],
          ["buzz-session-info", "1", transition, "thread", "2", "2", "exact"],
          ["buzz-session-action", "1", transition, command, actor],
        ],
      }),
    {
      root,
      command: old.id,
      transition,
      pubkey: AGENT,
      actor: VIEWER,
      channelName: CHANNEL,
    },
  );
  if (!response?.id) throw new Error("Manual transition not emitted");
  await expect(
    row(page, response.id).getByTestId("message-session-status"),
  ).toContainText("by npub1mock...");
  await waitForAnimations(page);
  await page
    .getByTestId("message-thread-panel")
    .screenshot({ path: `${SHOT}/expired-and-actor.png` });
});

test("owned observer activity projects typed compaction lifecycle through the real store and panel", async ({
  page,
}) => {
  const activityAgent = TEST_IDENTITIES.tyler.pubkey;
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: activityAgent,
        name: "Activity Agent",
        status: "running",
        channelNames: [CHANNEL],
      },
    ],
  });
  await page.goto("/");
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_SEED_OBSERVER_EVENTS__ === "function",
  );
  await page.getByTestId(`channel-${CHANNEL}`).click();
  const message = page
    .getByTestId("message-row")
    .filter({ has: page.getByText("Activity Agent", { exact: false }) })
    .first();
  await expect(message).toBeVisible();
  await message.getByRole("button").first().click();
  await expect(page.getByTestId("user-profile-panel")).toBeVisible();
  await page.getByTestId(`user-profile-view-activity-${activityAgent}`).click();
  const panel = page.getByTestId("agent-session-thread-panel");
  await expect(panel).toBeVisible();
  const base = {
    agentIndex: 0,
    channelId: "94a444a4-c0a3-5966-ab05-530c6ddc2301",
    sessionId: "activity-session",
    turnId: "compact-request",
  };
  const frame = (seq: number, kind: string, payload: unknown) => ({
    ...base,
    seq,
    timestamp: new Date(
      Date.parse("2026-10-01T12:00:00Z") + seq * 1000,
    ).toISOString(),
    kind,
    payload,
  });
  const typed = (
    seq: number,
    status: string,
    id = "compact-one",
    summary?: string,
  ) =>
    frame(seq, "acp_read", {
      method: "session/update",
      params: {
        sessionId: "activity-session",
        update: {
          sessionUpdate: "compaction_update",
          compactionId: id,
          status,
          ...(summary ? { summary } : {}),
        },
      },
    });
  const seed = async (
    events: Array<
      Omit<ReturnType<typeof frame>, "channelId" | "sessionId" | "turnId"> & {
        channelId: string | null;
        sessionId: string | null;
        turnId: string | null;
      }
    >,
  ) =>
    page.evaluate(
      ({ agentPubkey, events }) =>
        window.__BUZZ_E2E_SEED_OBSERVER_EVENTS__?.({ agentPubkey, events }),
      { agentPubkey: activityAgent, events },
    );
  const initial = typed(3, "in_progress");
  await seed([
    {
      ...frame(1, "agent_initialized", { agentIndex: 0, initializeResult: {} }),
      channelId: null,
      sessionId: null,
      turnId: null,
    },
    frame(2, "acp_write", {
      method: "session/prompt",
      params: {
        sessionId: "activity-session",
        prompt: [{ type: "text", text: "/compact" }],
      },
    }),
    initial,
  ]);
  const lifecycle = panel.getByTestId("transcript-lifecycle-item");
  await expect(lifecycle.filter({ hasText: "Compacting context" })).toHaveCount(
    1,
  );
  await expect(
    lifecycle.filter({ hasText: "Requested compaction" }),
  ).toHaveCount(1);
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOT}/activity-running-full.png` });
  await panel.screenshot({ path: `${SHOT}/activity-running-clip.png` });
  const terminal = typed(
    5,
    "failed",
    "compact-one",
    "Provider rejected compaction.",
  );
  await seed([
    frame(4, "acp_read", {
      method: "session/update",
      params: {
        sessionId: "activity-session",
        update: {
          sessionUpdate: "tool_call",
          toolCallId: "legacy-hook",
          title: "PostCompact",
          status: "completed",
        },
      },
    }),
    terminal,
    typed(
      6,
      "completed",
      "compact-two",
      "Second operation retained its outcome.",
    ),
  ]);
  await seed([initial, terminal]);
  await expect(
    lifecycle.filter({ hasText: "Context compaction failed" }),
  ).toHaveCount(1);
  await expect(lifecycle.filter({ hasText: "Context compacted" })).toHaveCount(
    1,
  );
  await expect(
    panel
      .getByTestId("transcript-tool-item")
      .filter({ hasText: "Context compacted" }),
  ).toHaveCount(0);
  const failure = lifecycle.filter({ hasText: "Context compaction failed" });
  await expect(failure).toHaveClass(/text-destructive/);
  await expect(failure).toContainText(
    "Adapter summary: Provider rejected compaction.",
  );
  await expect(panel.getByTestId("transcript-suppressed-item")).toHaveCount(0);
  const second = lifecycle.filter({
    hasText: "Second operation retained its outcome.",
  });
  await expect(second).toContainText("Compaction origin unknown");
  await expect(second).not.toContainText("Requested compaction");
  await seed([
    {
      ...frame(7, "agent_initialized", { agentIndex: 0, initializeResult: {} }),
      channelId: null,
      sessionId: null,
      turnId: null,
    },
    { ...typed(8, "in_progress"), turnId: "new-worker-turn" },
  ]);
  await expect(lifecycle.filter({ hasText: "Compacting context" })).toHaveCount(
    1,
  );
  await expect(failure).toHaveCount(1);
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOT}/activity-outcomes-full.png` });
  await panel.screenshot({ path: `${SHOT}/activity-outcomes-clip.png` });
  await panel.getByRole("button", { name: "Open activity settings" }).click();
  await page.getByTestId("agent-session-toggle-raw-feed").click();
  await page.keyboard.press("Escape");
  const raw = panel
    .locator("details")
    .filter({ has: page.locator("summary").filter({ hasText: "#5" }) });
  await raw.locator("summary").click();
  await expect(raw.locator("pre")).toContainText(
    '"compactionId": "compact-one"',
  );
  await expect(raw.locator("pre")).toContainText('"status": "failed"');
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOT}/activity-raw-full.png` });
  await panel.screenshot({ path: `${SHOT}/activity-raw-clip.png` });
});
