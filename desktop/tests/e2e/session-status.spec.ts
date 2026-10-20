import { expect, type Locator, type Page, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

// Agent session status rendering: an agent's own kind-9 messages carry
// signed `buzz-session` / `buzz-session-info` tags, and Desktop shows a
// small noninteractive line beneath the body. Ordinary mock-bridge events
// drive every case; no special test hooks are involved.

const CHANNEL = "agents";
const SHOT = "test-results/session-status";
// Must not collide with any e2eBridge constant.
const AGENT_PUBKEY = `5e55${"a".repeat(60)}`;
const T_NEW = "6f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";
const T_SLEEP = "0f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";
const T_THREAD = "1f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";

type EmittedEvent = { id: string };
type MockWindow = Window & {
  __BUZZ_E2E_EMIT_MOCK_MESSAGE__?: (input: {
    channelName: string;
    content: string;
    parentEventId?: string | null;
    pubkey?: string;
    extraTags?: string[][];
    createdAt?: number;
  }) => EmittedEvent;
  __BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?: (input: {
    channelName: string;
  }) => boolean;
};

function statusTags(
  state: string,
  reason: string,
  percent: string,
  transition: string,
  scope: "thread" | "conversation",
  history: [string, string, string] = ["", "", "unknown"],
) {
  return [
    ["buzz-session", state, reason, percent],
    ["buzz-session-info", "1", transition, scope, ...history],
  ];
}

async function emit(
  page: Page,
  input: {
    content: string;
    createdAt: number;
    pubkey?: string;
    parentEventId?: string;
    extraTags?: string[][];
  },
): Promise<string> {
  const event = await page.evaluate(
    (payload) =>
      (window as MockWindow).__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.(payload) ?? null,
    { channelName: CHANNEL, pubkey: AGENT_PUBKEY, ...input },
  );
  if (!event?.id) throw new Error("mock message emit did not return an id");
  return event.id;
}

function row(scope: Page | Locator, id: string) {
  return scope.locator(`[data-testid="message-row"][data-message-id="${id}"]`);
}

function statusOf(scope: Page | Locator, id: string) {
  return row(scope, id).getByTestId("message-session-status");
}

async function expectQuietRows(scope: Locator) {
  const attributes = await scope
    .getByTestId("message-session-status")
    .evaluateAll((rows) =>
      rows.flatMap((status) =>
        [status, ...status.querySelectorAll("*")].flatMap((element) =>
          ["tabindex", "aria-live", "aria-label", "role"].filter((name) =>
            element.hasAttribute(name),
          ),
        ),
      ),
    );
  expect(attributes).toEqual([]);
}

test.beforeEach(async ({ page }) => {
  await installMockBridge(page, {
    managedAgents: [],
    searchProfiles: [
      {
        pubkey: AGENT_PUBKEY,
        displayName: "StatusBot",
        ownerPubkey: "b4".repeat(32),
        isAgent: true,
      },
    ],
    relayAgents: [
      {
        pubkey: AGENT_PUBKEY,
        name: "StatusBot",
        ownerPubkey: "b4".repeat(32),
        respondTo: "allowlist",
        respondToAllowlist: ["deadbeef".repeat(8)],
        channelNames: [CHANNEL],
      },
    ],
  });
});

test("session status explains each transition once and shows reported context", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByTestId(`channel-${CHANNEL}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(CHANNEL);
  await expect
    .poll(() =>
      page.evaluate(
        (channelName) =>
          (window as MockWindow).__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
            channelName,
          }) ?? false,
        CHANNEL,
      ),
    )
    .toBe(true);

  const base = Math.floor(Date.now() / 1000) - 60;
  const first = await emit(page, {
    content: "First answer in a new session.",
    createdAt: base,
    extraTags: statusTags("new", "first_in_thread", "", T_NEW, "conversation", [
      "12",
      "37",
      "exact",
    ]),
  });
  const below = await emit(page, {
    content: "Continuation just below the threshold.",
    createdAt: base + 1,
    extraTags: statusTags(
      "new",
      "first_in_thread",
      "79",
      T_NEW,
      "conversation",
    ),
  });
  const at = await emit(page, {
    content: "Continuation at the threshold.",
    createdAt: base + 2,
    extraTags: statusTags(
      "new",
      "first_in_thread",
      "80",
      T_NEW,
      "conversation",
    ),
  });
  const resumed = await emit(page, {
    content: "Back after sleeping; context not reported yet.",
    createdAt: base + 3,
    extraTags: statusTags("resumed", "idle_sleep", "", T_SLEEP, "conversation"),
  });

  const timeline = page.getByTestId("message-timeline");
  const resumedRow = row(timeline, resumed);
  const scrollToLatest = page.getByTestId("message-scroll-to-latest");
  // Live arrivals can wait behind the timeline's pending-tail affordance.
  await expect
    .poll(
      async () =>
        (await resumedRow.isVisible()) || (await scrollToLatest.isVisible()),
    )
    .toBe(true);
  if (!(await resumedRow.isVisible())) {
    await expect(scrollToLatest).toBeVisible();
    await scrollToLatest.click();
  }
  await expect(resumedRow).toBeVisible();

  // New session: explanation with exact history, no fabricated percentage.
  await expect(statusOf(page, first)).toContainText("New session");
  await expect(statusOf(page, first)).toContainText("read 12 of 37 messages");
  await expect(statusOf(page, first)).not.toContainText("%");

  // Grouped continuation rows keep only their percentage; 79 is normal.
  await expect(row(page, below).getByTestId("message-header")).toHaveCount(0);
  await expect(statusOf(page, below)).toHaveText(/^79% context$/);
  await expect(
    statusOf(page, below).getByTestId("message-session-status-percent"),
  ).toHaveAttribute("data-level", "normal");

  // 80 is emphasized and says so in text.
  const high = statusOf(page, at).getByTestId("message-session-status-percent");
  await expect(high).toHaveAttribute("data-level", "high");
  await expect(high).toContainText("80% context (high)");
  await expect(statusOf(page, at)).not.toContainText("New session");

  // Unknown context: explanation only, no percentage or history claim.
  await expect(statusOf(page, resumed)).toContainText("Resumed after sleep");
  await expect(statusOf(page, resumed)).not.toContainText("%");
  await expect(statusOf(page, resumed)).not.toContainText("read");

  // One understandable explanation per loaded transition.
  await expect(timeline.getByText("New session", { exact: true })).toHaveCount(
    1,
  );
  await expect(
    timeline.getByText("Resumed after sleep", { exact: true }),
  ).toHaveCount(1);
  await expectQuietRows(timeline);

  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOT}/channel-status.png` });
  const statusGroup = statusOf(page, first).locator("..");
  await expect(
    statusGroup.getByRole("button", { name: "Manage current session" }),
  ).toHaveCount(0);
  await expect(page.getByTestId("manage-current-session")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Manage context" }),
  ).toHaveCount(0);
  await waitForAnimations(page);
  await statusGroup.screenshot({ path: `${SHOT}/quiet-status.png` });

  // Thread scope: a human root, two agent replies in one thread transition.
  const root = await emit(page, {
    content: "Can you look at the release notes?",
    createdAt: base + 4,
    pubkey: TEST_IDENTITIES.tyler.pubkey,
  });
  const reply1 = await emit(page, {
    content: "Looking now.",
    createdAt: base + 5,
    parentEventId: root,
    extraTags: statusTags(
      "resumed",
      "harness_restart",
      "45",
      T_THREAD,
      "thread",
    ),
  });
  const reply2 = await emit(page, {
    content: "Done, two typos fixed.",
    createdAt: base + 6,
    parentEventId: root,
    extraTags: statusTags(
      "resumed",
      "harness_restart",
      "46",
      T_THREAD,
      "thread",
    ),
  });

  await expect(row(page, root)).toBeVisible();
  await row(timeline, root).hover();
  await page.getByTestId(`reply-message-${root}`).click();
  const threadPanel = page.getByTestId("message-thread-panel");
  await expect(threadPanel).toBeVisible();

  await expect(statusOf(threadPanel, reply1)).toContainText(
    "Resumed after restart",
  );
  await expect(statusOf(threadPanel, reply1)).toContainText("45% context");
  await expect(statusOf(threadPanel, reply2)).toHaveText(/^46% context$/);
  await expect(
    threadPanel.getByText("Resumed after restart", { exact: true }),
  ).toHaveCount(1);
  // The human root never gets a status line.
  await expect(statusOf(threadPanel, root)).toHaveCount(0);
  await expectQuietRows(threadPanel);

  const context = threadPanel.getByRole("button", { name: "Manage context" });
  await expect(context).toBeVisible();
  await context.click();
  const agent = page.getByRole("menuitem", { name: "StatusBot", exact: true });
  await expect(agent).toBeVisible();
  await agent.click();
  await expect(
    page.getByRole("menuitem", { name: "Compact context", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("menuitem", { name: "Restart", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("menuitem", { name: "Back to agents", exact: true })
    .click();
  await expect(agent).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(context).toBeFocused();
  await expect(page.getByTestId("manage-current-session")).toHaveCount(0);

  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOT}/thread-status.png` });
});
