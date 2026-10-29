import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

// Mock agent pubkeys (distinct from the relay agents seeded by default).
const AGENT_PAUL = "aa".repeat(32);
const AGENT_DUNCAN = "bb".repeat(32);

// Mock channel IDs from the e2e bridge.
const CHANNEL_GENERAL = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const CHANNEL_ENGINEERING = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";

// A fixed epoch so the mocked clock is deterministic across runs.
const T0 = new Date("2026-06-18T12:00:00.000Z");

// Past both thresholds: FRAME_GAP_PAUSE_MS (20s) and REMOVE_AFTER_MS (25s).
// Several 5s prune ticks fire across this span, so shouldPausePrune is what
// keeps the badges alive — not the absence of a prune tick.
const FRAME_GAP_MS = 30_000;

type SeedInput = {
  agentPubkey: string;
  channelId: string;
  turnId: string;
};

async function waitForBridge(page: import("@playwright/test").Page) {
  await page.waitForFunction(
    () =>
      typeof (window as Window & { __BUZZ_E2E_SEED_ACTIVE_TURNS__?: unknown })
        .__BUZZ_E2E_SEED_ACTIVE_TURNS__ === "function",
    null,
    { timeout: 10_000 },
  );
}

async function openAgentsView(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await waitForBridge(page);
  await page.getByTestId("open-agents-view").click();
  await expect(page.getByTestId("unified-agents-groups")).toBeVisible({
    timeout: 10_000,
  });
}

async function seedTurns(
  page: import("@playwright/test").Page,
  turns: SeedInput[],
) {
  await page.evaluate((seeds) => {
    const win = window as Window & {
      __BUZZ_E2E_SEED_ACTIVE_TURNS__?: (input: {
        agentPubkey: string;
        channelId: string;
        turnId: string;
      }) => void;
    };
    for (const seed of seeds) win.__BUZZ_E2E_SEED_ACTIVE_TURNS__?.(seed);
  }, turns);
}

async function openAgentProfile(
  page: import("@playwright/test").Page,
  pubkey: string,
) {
  await page.getByTestId(`managed-agent-${pubkey}`).click();
  const panel = page.getByTestId("user-profile-panel");
  await expect(panel).toBeVisible({ timeout: 5_000 });
  return panel;
}

test.describe("active turn badge resilience", () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  test("thread composer follows scoped observer work when typing disappears", async ({
    page,
  }, testInfo) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PAUL,
          name: "Paul",
          status: "running",
          channelNames: ["general"],
        },
      ],
    });
    test.setTimeout(60_000);
    // Keep real browser time: WAAPI exit animations do not follow the mock clock.
    await page.goto("/");
    await page.getByTestId("channel-general").click();
    await waitForBridge(page);
    await expect
      .poll(() =>
        page.evaluate(() =>
          window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
            channelName: "general",
          }),
        ),
      )
      .toBe(true);
    const root = await page.evaluate(
      (pubkey) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "general",
          pubkey,
          content: "Investigate thread activity",
        })?.id,
      AGENT_PAUL,
    );
    if (!root) throw new Error("Missing test thread root");
    const row = page.locator(
      `[data-testid="message-row"][data-message-id="${root}"]`,
    );
    await expect(row).toBeVisible();
    await row.hover();
    const reply = page.getByTestId(`reply-message-${root}`);
    await reply.focus();
    await reply.press("Enter");
    const composer = page.getByTestId("thread-composer-overlay");
    await expect(composer).toBeVisible();
    const activity = composer.getByTestId("bot-activity-composer-trigger");
    let seq = 0;
    const emit = async (
      kind: string,
      turnId: string,
      threadRootEventId?: string,
    ) => {
      await page.evaluate(
        ({ kind, turnId, threadRootEventId, seq, agentPubkey, channelId }) => {
          window.__BUZZ_E2E_SEED_OBSERVER_EVENTS__?.({
            agentPubkey,
            events: [
              {
                seq,
                timestamp: new Date().toISOString(),
                kind,
                channelId,
                agentIndex: 0,
                sessionId: null,
                turnId,
                payload: { threadRootEventId },
              },
            ],
          });
        },
        {
          kind,
          turnId,
          threadRootEventId,
          seq: ++seq,
          agentPubkey: AGENT_PAUL,
          channelId: CHANNEL_GENERAL,
        },
      );
    };
    await emit("turn_started", "sibling", "ef".repeat(32));
    await expect(page.getByTestId("channel-working-general")).toBeVisible();
    await expect(activity).toHaveCount(0);
    await emit("acp_read", "current");
    await expect(activity).toHaveCount(0); // Unknown scope is channel-only.
    await emit("turn_liveness", "current", root);
    await expect(activity).toHaveAccessibleName(
      "Paul is working. View activity.",
    );

    // A published reply must not clear observer work, even with no typing.
    await page.evaluate(
      ({ root, pubkey }) => {
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "general",
          pubkey,
          parentEventId: root,
          content: "Still investigating",
        });
      },
      { root, pubkey: AGENT_PAUL },
    );
    await page.waitForTimeout(9_000); // Past the typing TTL.
    await emit("turn_liveness", "current", root);
    await expect(activity).toBeVisible();
    await composer.screenshot({
      path: testInfo.outputPath("thread-observer-activity.png"),
    });
    await page.screenshot({
      path: testInfo.outputPath("thread-observer-activity-full.png"),
    });
    await emit("turn_completed", "current", root);
    await expect(activity).toHaveCount(0);
    await expect(page.getByTestId("channel-working-general")).toBeVisible();

    // Older/remote harnesses can still show scoped typing without telemetry.
    await page.evaluate(
      ({ root, pubkey }) => {
        window.__BUZZ_E2E_EMIT_MOCK_TYPING__?.({
          channelName: "general",
          pubkey,
          threadHeadId: root,
        });
      },
      { root, pubkey: AGENT_PAUL },
    );
    await expect(activity).toBeVisible();
    await page.waitForTimeout(9_000);
    await expect(activity).toHaveCount(0);
    await emit("turn_error", "sibling", "ef".repeat(32));
    await expect(page.getByTestId("channel-working-general")).toHaveCount(0);
  });

  test("badges persist through an all-at-once liveness gap", async ({
    page,
  }) => {
    // Install the mocked clock BEFORE navigation so the store's Date.now() /
    // setInterval and the badge's useNow(1000) all run on the mocked clock from
    // module init. Seeded turns then stamp lastActivityAt at T0.
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PAUL,
          name: "Paul",
          status: "running",
          channelNames: ["general", "engineering"],
        },
        {
          pubkey: AGENT_DUNCAN,
          name: "Duncan",
          status: "running",
          channelNames: ["general"],
        },
      ],
    });
    await page.clock.install({ time: T0 });

    await openAgentsView(page);

    // Both agents working across channels — the healthy multi-agent state.
    await seedTurns(page, [
      {
        agentPubkey: AGENT_PAUL,
        channelId: CHANNEL_GENERAL,
        turnId: "t-paul-g",
      },
      {
        agentPubkey: AGENT_PAUL,
        channelId: CHANNEL_ENGINEERING,
        turnId: "t-paul-e",
      },
      {
        agentPubkey: AGENT_DUNCAN,
        channelId: CHANNEL_GENERAL,
        turnId: "t-duncan-g",
      },
    ]);

    const paulPanel = await openAgentProfile(page, AGENT_PAUL);
    await expect(paulPanel).toBeVisible();

    // The profile panel surfaces active turns via the live-activity embed only
    // where an agent session can open (channel surfaces). In the Agents view
    // the store-driven working state shows as sidebar channel badges — the
    // same activeAgentTurnsStore this test exercises.
    const generalBadge = page.getByTestId("channel-working-general");
    const engineeringBadge = page.getByTestId("channel-working-engineering");
    await expect(generalBadge).toBeVisible({ timeout: 5_000 });
    await expect(engineeringBadge).toBeVisible();

    // Simulate the all-at-once relay drop: no further frames, advance the clock
    // past both thresholds. This fires several real prune ticks; shouldPausePrune
    // sees every turn's lastActivityAt stuck at T0 (gap > 20s) and pauses the
    // prune, so the active-turn-driven working badges survive. Under the
    // pre-fix code every badge would be gone after the first tick past 25s.
    await page.clock.fastForward(FRAME_GAP_MS);

    await expect(generalBadge).toBeVisible();
    await expect(engineeringBadge).toBeVisible();
  });
});
