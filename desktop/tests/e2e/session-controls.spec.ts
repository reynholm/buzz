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
  sendMessageDelayMs = 0,
  mockOptions: Parameters<typeof installMockBridge>[1] = {},
) {
  await installMockBridge(page, {
    ...mockOptions,
    sendMessageDelayMs,
    ownerOnlyAccessBuild: true,
    managedAgents: [],
    relayAgentRevalidationRevokedPubkeys: revoked ? [AGENT] : [],
    searchProfiles: [
      {
        pubkey: AGENT,
        displayName: "ControlBot",
        avatarUrl:
          "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='32' height='32'%3E%3Crect width='32' height='32' fill='%233b82f6'/%3E%3Cpath d='M8 10h16v14H8z' fill='white'/%3E%3C/svg%3E",
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
function row(page: Page | Locator, id: string) {
  return page.locator(`[data-testid="message-row"][data-message-id="${id}"]`);
}
async function openThread(page: Page, root: string) {
  await row(page, root).hover();
  const reply = page.getByTestId(`reply-message-${root}`);
  await reply.focus();
  await reply.press("Enter");
  await expect(page.getByTestId("message-thread-panel")).toBeVisible();
}
async function selectAgent(page: Page, name = "ControlBot") {
  const trigger = page.getByTestId("manage-thread-context");
  await trigger.click();
  const agent = page.getByRole("menuitem", { name, exact: true });
  const avatar = agent.getByTestId("thread-context-agent-avatar");
  await expect(avatar).toHaveAttribute("data-avatar-shape", "squircle");
  await expect(avatar.locator("..")).toHaveAttribute("aria-hidden", "true");
  if (name === "ControlBot") {
    await expect(
      agent.getByTestId("thread-context-agent-avatar-image"),
    ).toBeVisible();
  } else {
    await expect(
      agent.getByTestId("thread-context-agent-avatar-fallback"),
    ).toBeVisible();
  }
  await expectContextAboveComposer(page);
  await agent.click();
  return trigger;
}
async function expectContextAboveComposer(page: Page) {
  const form = page
    .getByTestId("thread-composer-overlay")
    .getByTestId("message-composer");
  const menu = page.getByRole("menu").first();
  await expect(menu).toHaveAttribute("data-side", "top");
  await expect
    .poll(async () => {
      const [field, popup] = await Promise.all([
        form.boundingBox(),
        menu.boundingBox(),
      ]);
      return field && popup ? field.y - popup.y - popup.height : -1;
    })
    .toBeGreaterThanOrEqual(3);
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
async function composerSelection(input: Locator) {
  return input.evaluate((element) => {
    const selection = window.getSelection();
    if (
      !selection?.anchorNode ||
      !selection.focusNode ||
      !element.contains(selection.anchorNode) ||
      !element.contains(selection.focusNode)
    )
      return null;
    const offset = (node: Node, position: number) => {
      const range = document.createRange();
      range.selectNodeContents(element);
      range.setEnd(node, position);
      return range.toString().length;
    };
    return {
      anchor: offset(selection.anchorNode, selection.anchorOffset),
      focus: offset(selection.focusNode, selection.focusOffset),
      text: selection.toString(),
    };
  });
}
async function expectContextBeforeMentions(page: Page) {
  const composer = page.getByTestId("thread-composer-overlay");
  const context = composer.getByTestId("manage-thread-context");
  const mention = composer.getByTestId("message-insert-mention");
  await expect(context).toBeVisible();
  await expect(context).toBeInViewport();
  await expect(mention).toBeVisible();
  await expect(mention).toBeInViewport();
  expect(
    await context.evaluate((element) =>
      Boolean(
        element.nextElementSibling?.querySelector(
          '[data-testid="message-insert-mention"]',
        ),
      ),
    ),
  ).toBe(true);
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
  await expect(
    page.getByRole("button", { name: "Manage context" }),
  ).toHaveCount(0);
  await openThread(page, root);
  const trigger = page.getByTestId("manage-thread-context");
  await expect(trigger).toBeVisible();
  await expect(trigger).toHaveAccessibleName("Manage context");
  await expect(trigger.locator("svg.lucide-brain-cog")).toHaveAttribute(
    "aria-hidden",
    "true",
  );
  const mention = page
    .getByTestId("thread-composer-overlay")
    .getByTestId("message-insert-mention");
  expect(
    await trigger.evaluate((element) =>
      element.nextElementSibling?.contains(
        document.querySelector(
          '[data-testid="thread-composer-overlay"] [data-testid="message-insert-mention"]',
        ),
      ),
    ),
  ).toBe(true);
  await expect(mention).toBeVisible();
  await expect(page.getByTestId("manage-current-session")).toHaveCount(0);
  await waitForAnimations(page);
  const threadHead = row(
    page.getByTestId("message-thread-panel").getByTestId("message-thread-head"),
    root,
  );
  await threadHead.screenshot({ path: `${SHOT}/before-controls.png` });
  await expect(threadHead.getByTestId("message-session-status")).toHaveCount(0);
  const draft = page
    .getByTestId("thread-composer-overlay")
    .getByTestId("message-input");
  await draft.fill("Preserve this unfinished request");
  await trigger.focus();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("menuitem", { name: "ControlBot", exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("menuitem", { name: "ControlBot", exact: true })
      .getByTestId("thread-context-agent-avatar-image"),
  ).toBeVisible();
  await captureMenu(page, trigger, "agent-list-avatar-v1");
  await expectContextAboveComposer(page);
  expect(await sends(page)).toHaveLength(0);
  await page.getByRole("menuitem", { name: "ControlBot", exact: true }).click();
  await expect(
    page.getByRole("menuitem", { name: "Compact context", exact: true }),
  ).toBeVisible();
  expect(await sends(page)).toHaveLength(0);
  const restart = page.getByRole("menuitem", { name: "Restart", exact: true });
  const restartDescription =
    "Stops current work, including subagents. File changes are kept.";
  await expect(
    restart.getByText(restartDescription, { exact: true }),
  ).toBeVisible();
  await expect(restart).toHaveAccessibleDescription(restartDescription);
  await expectDescriptionStyle(restart);
  await captureMenu(page, trigger, "menu-v3");
  await restart.focus();
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
  await captureMenu(page, trigger, "restart-submenu-v3");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(trigger).toBeFocused();
  expect(await sends(page)).toHaveLength(0);
  await selectAgent(page);
  await page.getByRole("menuitem", { name: "Restart", exact: true }).click();
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
  await expect(draft).toHaveText(
    "@ControlBot Preserve this unfinished request",
  );
  await expect(page.getByTestId("message-session-control")).toContainText(
    "Start over from scratch",
  );
  await expect(page.getByTestId("message-session-control")).toContainText(
    "Waiting for agent…",
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
  await page
    .getByTestId("message-thread-panel")
    .screenshot({ path: `${SHOT}/request-with-mentions.png` });
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
  await draft.fill("@ControlBot Continue the task");
  await page
    .getByTestId("thread-composer-overlay")
    .getByTestId("send-message")
    .click();
  await page
    .getByRole("alertdialog", { name: "Mention people outside this channel?" })
    .getByRole("button", { name: "Invite", exact: true })
    .click();
  const sentRow = page
    .getByTestId("message-thread-panel")
    .getByTestId("message-row")
    .filter({ hasText: "Continue the task" });
  await expect(sentRow).toHaveCount(1);
  await expect(sentRow).toHaveAttribute("data-message-id", /^[0-9a-f]{64}$/);
  const sentId = await sentRow.getAttribute("data-message-id");
  const sentEvent = await page.evaluate(
    async (eventId) =>
      JSON.parse(
        String(
          await window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("get_event", {
            eventId,
          }),
        ),
      ),
    sentId,
  );
  expect(sentEvent.tags.filter((tag: string[]) => tag[0] === "p")).toEqual([
    ["p", AGENT],
  ]);
});
test("revoked send-time permission preserves draft and publishes nothing", async ({
  page,
}) => {
  const root = await setup(page, true);
  await openThread(page, root);
  const draft = page
    .getByTestId("thread-composer-overlay")
    .getByTestId("message-input");
  await draft.fill("Keep my draft");
  await selectAgent(page);
  await page
    .getByRole("menuitem", { name: "Compact context", exact: true })
    .click();
  await expect(
    page.getByTestId("thread-composer-overlay").getByRole("alert"),
  ).toContainText("Could not send session control");
  expect(await sends(page)).toHaveLength(0);
  await expect(draft).toHaveText("Keep my draft");
});

test("restart keeps ordinary activity below the composer until the status reply", async ({
  page,
}) => {
  const root = await setup(page);
  await openThread(page, root);
  const composer = page.getByTestId("thread-composer-overlay");
  const input = composer.getByTestId("message-input");
  await selectAgent(page);
  await page.getByRole("menuitem", { name: "Restart", exact: true }).click();
  await page
    .getByRole("menuitem", { name: "Recent history", exact: true })
    .click();
  const control = page.getByTestId("message-session-control");
  await expect(control).toBeVisible();
  await expect(input).toHaveText("@ControlBot ");
  const commandId = await page
    .getByTestId("message-row")
    .filter({ has: control })
    .getAttribute("data-message-id");
  if (!commandId) throw new Error("Missing restart event identity");
  const heartbeat = await page.evaluate(
    ({ root, commandId, channelName, pubkey }) => {
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        pubkey,
        kind: 7,
        content: "✅",
        extraTags: [
          ["e", commandId],
          ["buzz-session-result", "1", commandId, "ok", ""],
        ],
      });
      const emit = () =>
        window.__BUZZ_E2E_EMIT_MOCK_TYPING__?.({
          channelName,
          pubkey,
          threadHeadId: root,
        });
      emit();
      return window.setInterval(emit, 3000);
    },
    { root, commandId, channelName: CHANNEL, pubkey: AGENT },
  );
  await expect(control).toContainText("Completed");
  const activity = composer.getByTestId("bot-activity-composer-trigger");
  await expect(activity).toHaveAccessibleName(
    "ControlBot is working. View activity.",
  );
  // Keep the normal heartbeat beyond one client typing TTL (eight seconds).
  await page.waitForTimeout(9000);
  await expect(activity).toBeVisible();
  const [form, rail] = await Promise.all([
    composer.getByTestId("message-composer").boundingBox(),
    activity.boundingBox(),
  ]);
  if (!form || !rail) throw new Error("Missing composer activity geometry");
  expect(rail.y).toBeGreaterThanOrEqual(form.y + form.height);
  await page.mouse.move(0, 0);
  await input.focus();
  await waitForAnimations(page);
  await composer.screenshot({ path: `${SHOT}/restart-awaiting-status.png` });
  await page.evaluate(
    ({ heartbeat, root, channelName, pubkey }) => {
      window.clearInterval(heartbeat);
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        pubkey,
        parentEventId: root,
        content: "The session restarted. Validation is the next step.",
      });
    },
    { heartbeat, root, channelName: CHANNEL, pubkey: AGENT },
  );
  await expect(activity).toHaveCount(0);
  await expect(input).toHaveText("@ControlBot ");
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
    await selectAgent(page);
    if (label !== "Compact context")
      await page
        .getByRole("menuitem", { name: "Restart", exact: true })
        .click();
    await page.getByRole("menuitem", { name: label, exact: true }).click();
    await expect.poll(async () => (await sends(page)).length).toBe(1);
    const payload = (await sends(page))[0].payload as Record<string, unknown>;
    expect(payload.content).toBe(body);
    expect(payload.parentEventId).toBe(root);
    expect(payload.rootEventId).toBe(root);
    expect(payload.mentionPubkeys).toEqual([AGENT]);
    await expect(
      page
        .getByTestId("thread-composer-overlay")
        .getByTestId(`composer-address-lock-${AGENT}`),
    ).toBeVisible();
    await expect(
      page
        .getByTestId("message-session-control")
        .getByTestId("session-control-progress"),
    ).toBeVisible();
    if (body === "!compact") {
      const control = page.getByTestId("message-session-control");
      const commandId = await page
        .getByTestId("message-row")
        .filter({ has: control })
        .getAttribute("data-message-id");
      if (!commandId) throw new Error("Missing compact event identity");
      for (const outcome of ["pending", "ok"]) {
        await page.evaluate(
          ({ commandId, outcome, pubkey, channelName }) =>
            window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
              channelName,
              pubkey,
              kind: 7,
              content: outcome === "pending" ? "⏳" : "✅",
              extraTags: [
                ["e", commandId],
                ["buzz-session-result", "1", commandId, outcome, ""],
              ],
            }),
          { commandId, outcome, pubkey: AGENT, channelName: CHANNEL },
        );
        await expect(control).toContainText(
          outcome === "pending"
            ? "Waiting for current work to finish · Compaction will start automatically"
            : "Completed",
        );
        await expect(
          control.getByTestId("session-control-progress"),
        ).toHaveCount(outcome === "pending" ? 1 : 0);
      }
    }
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
  await openThread(page, second.id);
  await page.getByTestId("manage-thread-context").click();
  await expect(
    page.getByRole("menuitem", { name: "ControlBot", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("menuitem", { name: "SecondBot", exact: true }).click();
  await page.getByRole("menuitem", { name: "Restart", exact: true }).click();
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
  await openThread(page, root);
  await page.getByTestId("manage-thread-context").click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "No agents you can manage in this thread." }),
  ).toBeVisible();
  await expect(page.getByRole("menuitem")).toHaveCount(0);
  expect(await sends(page)).toHaveLength(0);
});

test("context controls remain available while sending disables the thread editor", async ({
  page,
}) => {
  const root = await setup(page, false, false, 0, 5_000);
  await openThread(page, root);
  const composer = page.getByTestId("thread-composer-overlay");
  const editor = composer.getByTestId("message-input");
  await editor.fill("Hold this send while managing context");
  await composer.getByTestId("send-message").click();
  await expect(editor).toHaveAttribute("contenteditable", "false");
  await expect(page.getByTestId("manage-thread-context")).toBeEnabled();
  await selectAgent(page);
  const compact = page.getByRole("menuitem", {
    name: "Compact context",
    exact: true,
  });
  await expect(compact).toBeEnabled();
  await compact.click();
  const trigger = page.getByTestId("manage-thread-context");
  await expect(trigger).toHaveAttribute("aria-busy", "true");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await trigger.focus();
  await trigger.press("Enter");
  await expect(
    page.getByRole("menuitem", { name: "Compact context", exact: true }),
  ).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect.poll(async () => (await sends(page)).length).toBe(2);
  const payload = (await sends(page))[1].payload as Record<string, unknown>;
  expect(payload.content).toBe("!compact");
  expect(payload.mentionPubkeys).toEqual([AGENT]);
  expect(payload.rootEventId).toBe(root);
  expect(payload.parentEventId).toBe(root);
  await expect(trigger).toHaveAttribute("aria-busy", "false", {
    timeout: 10_000,
  });
  await expect(
    composer.getByTestId(`composer-address-lock-${AGENT}`),
  ).toBeVisible();
});

test("context action pins its agent while preserving the draft, attachment, selection and caret", async ({
  page,
}) => {
  const root = await setup(page, false, false, 0, 0, {
    deferredComposerUploads: true,
    uploadDescriptors: [
      {
        url: `https://mock.relay/media/${"a".repeat(64)}.pdf`,
        sha256: "a".repeat(64),
        size: 12345,
        type: "application/pdf",
        uploaded: 0,
        filename: "context-draft.pdf",
      },
    ],
  });
  await openThread(page, root);
  const composer = page.getByTestId("thread-composer-overlay");
  const input = composer.getByTestId("message-input");
  const mention = composer.getByTestId("message-insert-mention");
  await mention.click();
  await composer
    .getByTestId("mention-autocomplete")
    .getByRole("button", { name: "Automatically mention ControlBot" })
    .click();
  await expect(
    composer.getByTestId(`composer-address-lock-${AGENT}`),
  ).toBeVisible();
  await mention.click();
  await composer.getByTestId(`composer-address-lock-remove-${AGENT}`).click();
  await expect(
    composer.getByTestId(`composer-address-lock-${AGENT}`),
  ).toHaveCount(0);
  await input.fill("Keep this draft");
  const [chooser] = await Promise.all([
    page.waitForEvent("filechooser"),
    composer.getByRole("button", { name: "Attach file" }).click(),
  ]);
  await chooser.setFiles({
    buffer: Buffer.from("context attachment"),
    mimeType: "application/pdf",
    name: "context-draft.pdf",
  });
  await expect(composer).toContainText("context-draft.pdf");
  await expect(input).toHaveText("Keep this draft");
  await input.focus();
  await expect(input).toBeFocused();
  await input.evaluate((element) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const node = walker.currentNode;
      const index = (node.textContent ?? "").indexOf("this");
      if (index < 0) continue;
      const range = document.createRange();
      range.setStart(node, index);
      range.setEnd(node, index + "this".length);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      document.dispatchEvent(new Event("selectionchange"));
      return;
    }
    throw new Error("Could not establish the initial draft selection");
  });
  const selection = { anchor: 5, focus: 9, text: "this" };
  await expect.poll(() => composerSelection(input)).toEqual(selection);
  await selectAgent(page);
  expect(await sends(page)).toHaveLength(0);
  await page.keyboard.press("Escape");
  await input.focus();
  await expect.poll(() => composerSelection(input)).toEqual(selection);
  await input.evaluate(() => {
    window.getSelection()?.collapseToEnd();
    document.dispatchEvent(new Event("selectionchange"));
  });
  const caret = { anchor: 9, focus: 9, text: "" };
  await expect.poll(() => composerSelection(input)).toEqual(caret);
  await selectAgent(page);
  await page
    .getByRole("menuitem", { name: "Compact context", exact: true })
    .click();
  await expect.poll(async () => (await sends(page)).length).toBe(1);
  const payload = (await sends(page))[0].payload as Record<string, unknown>;
  expect(payload.content).toBe("!compact");
  expect(payload.mentionPubkeys).toEqual([AGENT]);
  expect(payload.mediaTags ?? []).toEqual([]);
  await expect(input).toHaveText("@ControlBot Keep this draft");
  await expect(composer).toContainText("context-draft.pdf");
  await expect(
    composer.getByTestId(`composer-address-lock-${AGENT}`),
  ).toBeVisible();
  await expect(mention).toHaveAccessibleName("Manage mentions");
  await input.focus();
  await expect
    .poll(() => composerSelection(input))
    .toEqual({
      anchor: caret.anchor + "@ControlBot ".length,
      focus: caret.focus + "@ControlBot ".length,
      text: "",
    });
});

test("context stays before mentions across formatting, focus and narrow thread layouts", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.addInitScript(() =>
    localStorage.setItem("buzz.channels.threadViewMode", "split"),
  );
  const root = await setup(page);
  await openThread(page, root);
  const composer = page.getByTestId("thread-composer-overlay");
  const input = composer.getByTestId("message-input");
  await input.fill("Keep my layout draft");
  await expectContextBeforeMentions(page);
  await composer.getByRole("button", { name: "Toggle formatting" }).click();
  await expect(
    composer.getByRole("button", { name: "Close formatting" }),
  ).toBeVisible();
  await expectContextBeforeMentions(page);
  await composer.getByTestId("manage-thread-context").focus();
  await page.keyboard.press("Space");
  const agent = page.getByRole("menuitem", { name: "ControlBot", exact: true });
  await expect(agent).toBeVisible();
  await expectContextAboveComposer(page);
  await page.keyboard.press("ArrowDown");
  await expect(agent).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("menuitem", { name: "Compact context", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(composer.getByTestId("manage-thread-context")).toBeFocused();
  await composer.getByRole("button", { name: "Close formatting" }).click();
  await page
    .getByRole("button", { name: "Expand thread", exact: true })
    .click();
  await expect(page.getByTestId("focus-thread-drawer")).toBeVisible();
  await expectContextBeforeMentions(page);
  await selectAgent(page);
  await expectContextAboveComposer(page);
  await composer.getByTestId("manage-thread-context").click();
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(input).toHaveText("Keep my layout draft");
  await page
    .getByRole("button", { name: "Show thread beside channel", exact: true })
    .click();
  await expect(page.getByTestId("focus-thread-drawer")).toHaveCount(0);
  await page.setViewportSize({ width: 860, height: 720 });
  await expect(page.getByTestId("thread-view-mode-toggle")).toHaveCount(0);
  await expectContextBeforeMentions(page);
  await composer.getByTestId("manage-thread-context").click();
  await expect(agent).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(input).toHaveText("Keep my layout draft");
  expect(await sends(page)).toHaveLength(0);
});

test.describe("touch context navigation", () => {
  test.use({ hasTouch: true });
  test("touch selection and Back retain the first agent-list step without sending", async ({
    page,
  }) => {
    const root = await setup(page);
    await openThread(page, root);
    const trigger = page.getByTestId("manage-thread-context");
    await trigger.tap();
    await page.getByRole("menuitem", { name: "ControlBot", exact: true }).tap();
    await expect(
      page.getByRole("menuitem", { name: "Compact context", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("menuitem", { name: "Back to agents", exact: true })
      .tap();
    await expect(
      page.getByRole("menuitem", { name: "ControlBot", exact: true }),
    ).toBeVisible();
    expect(await sends(page)).toHaveLength(0);
    await page.keyboard.press("Escape");
    await expect(trigger).toBeFocused();
  });
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
  const summary = page.locator(
    `[data-testid="message-thread-summary"][data-thread-head-id="${root}"]`,
  );
  await expect(summary).toContainText("2 replies");
  await summary.click();
  await expect(page.getByTestId("message-thread-panel")).toBeVisible();
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

for (const [name, response, eventId] of [
  [
    "restart_acknowledgment_is_distinct_from_generated_status",
    "The assignment is understood. Validation is still open; the next step is to run the focused checks.",
    "d8".repeat(32),
  ],
  [
    "restart_success_and_followup_failure_remain_independent",
    "The session restarted, but I couldn't confirm its task-status reply. Your next message can continue the conversation.",
    "e8".repeat(32),
  ],
] as const) {
  test(name, async ({ page }) => {
    const root = await setup(page);
    const sibling = await page.evaluate(
      ({ channelName, pubkey }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          pubkey,
          content: "An independent sibling thread stays unchanged.",
        }),
      { channelName: CHANNEL, pubkey: SECOND_AGENT },
    );
    if (!sibling?.id) throw new Error("Sibling event not emitted");
    await openThread(page, root);
    const panel = page.getByTestId("message-thread-panel");
    const draft = page
      .getByTestId("thread-composer-overlay")
      .getByTestId("message-input");
    await draft.fill("Preserve the next real task instruction");
    await selectAgent(page);
    await page.getByRole("menuitem", { name: "Restart", exact: true }).click();
    await page
      .getByRole("menuitem", { name: "From scratch", exact: true })
      .click();
    await expect.poll(async () => (await sends(page)).length).toBe(1);
    const payload = (await sends(page))[0].payload as Record<string, unknown>;
    expect(payload.content).toBe("!restart none");
    expect(payload.mentionPubkeys).toEqual([AGENT]);
    expect(payload.parentEventId).toBe(root);
    expect(payload.rootEventId).toBe(root);
    expect(payload.expectedSignerPubkey).toBe(VIEWER);
    expect(payload.expectedRelayUrl).toBeTruthy();
    const control = panel.getByTestId("message-session-control");
    const command = panel
      .getByTestId("message-row")
      .filter({ has: page.getByTestId("message-session-control") });
    await expect(command).toHaveCount(1);
    const commandId = await command.getAttribute("data-message-id");
    if (!commandId) throw new Error("Missing restart event identity");
    await expect(control).toContainText("Waiting for agent…");
    await page.evaluate(
      ({ channelName, pubkey, commandId }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          pubkey,
          kind: 7,
          content: "✅",
          extraTags: [
            ["e", commandId],
            ["buzz-session-result", "1", commandId, "ok", ""],
          ],
        }),
      { channelName: CHANNEL, pubkey: AGENT, commandId },
    );
    // A reset acknowledgment proves no model answer or provider readiness.
    await expect(control).toContainText("Completed");
    await expect(panel.getByText(response, { exact: true })).toHaveCount(0);
    await expect(
      panel.getByText(/session is ready|ready to continue/i),
    ).toHaveCount(0);
    await expect(draft).toHaveText(
      "@ControlBot Preserve the next real task instruction",
    );
    const createdAt = Math.floor(Date.now() / 1000);
    const input = {
      channelName: CHANNEL,
      pubkey: AGENT,
      kind: 9,
      content: response,
      extraTags: [["e", root, "", "reply"]],
      id: eventId,
      createdAt,
    };
    await page.evaluate(
      (input) => window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.(input),
      input,
    );
    await expect(row(panel, eventId)).toHaveCount(1);
    await expect(row(panel, eventId)).toContainText(response);
    // Replay the same mock event to bind ordinary timeline deduplication.
    await page.evaluate(
      (input) => window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.(input),
      input,
    );
    await expect(row(panel, eventId)).toHaveCount(1);
    await expect(control).toContainText("Completed");
    await expect(control).not.toContainText("Failed");
    await expect(draft).toHaveText(
      "@ControlBot Preserve the next real task instruction",
    );
    const raw = await page.evaluate(
      async (eventId) =>
        JSON.parse(
          String(
            await window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("get_event", {
              eventId,
            }),
          ),
        ),
      eventId,
    );
    expect(raw.kind).toBe(9);
    expect(raw.pubkey).toBe(AGENT);
    expect(raw.content).toBe(response);
    expect(raw.tags.filter((tag: string[]) => tag[0] === "p")).toEqual([]);
    expect(raw.tags.filter((tag: string[]) => tag[0] === "broadcast")).toEqual(
      [],
    );
    expect(
      raw.tags
        .filter((tag: string[]) => tag[0] === "e")
        .map((tag: string[]) => tag[1]),
    ).toContain(root);
    expect(
      raw.tags
        .filter((tag: string[]) => tag[0] === "e")
        .map((tag: string[]) => tag[1]),
    ).not.toContain(sibling.id);
    await expect(row(page, sibling.id)).not.toContainText(response);
    expect(await sends(page)).toHaveLength(1);
    if (name === "restart_success_and_followup_failure_remain_independent") {
      await expect(
        panel.getByText("Validation is complete and the agent is ready.", {
          exact: true,
        }),
      ).toHaveCount(0);
    }
  });
}
