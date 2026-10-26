import { expect, test, type Locator, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const CHANNEL = "general";
const ROOT_ID = "mock-general-welcome";
const SHOTS = "test-results/open-thread-highlight";

async function openChannel(page: Page, theme: "buzz" | "buzz-dark") {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.addInitScript((value) => {
    window.localStorage.setItem("buzz-theme", value);
  }, theme);
  await installMockBridge(page);
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("data-buzz-theme", theme);
  await page.getByTestId("channel-general").click();
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
}

async function expectNoHighlight(page: Page) {
  await expect(page.locator('[data-open-thread-head="true"]')).toHaveCount(0);
  await expect(
    page.locator('[data-testid="message-timeline"] [aria-current]'),
  ).toHaveCount(0);
}

async function expectStableBackground(marked: Locator) {
  const background = await marked.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  const root = marked.locator(`[data-message-id="${ROOT_ID}"]`);
  const rootBackground = await root.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  expect(background).not.toBe("rgba(0, 0, 0, 0)");
  await marked.hover();
  await expect
    .poll(() =>
      marked.evaluate((element) => getComputedStyle(element).backgroundColor),
    )
    .toBe(background);
  await expect
    .poll(() =>
      root.evaluate((element) => getComputedStyle(element).backgroundColor),
    )
    .toBe(rootBackground);
  await root.getByRole("button", { name: "Reply" }).focus();
  await expect
    .poll(() =>
      marked.evaluate((element) => getComputedStyle(element).backgroundColor),
    )
    .toBe(background);
  await expect
    .poll(() =>
      root.evaluate((element) => getComputedStyle(element).backgroundColor),
    )
    .toBe(rootBackground);
}

for (const theme of ["buzz", "buzz-dark"] as const) {
  test(`${theme}: highlights only the open root with a reply summary`, async ({
    page,
  }) => {
    await openChannel(page, theme);
    await page.evaluate(
      ({ channelName, parentEventId, pubkey }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          content: "Reply to welcome",
          parentEventId,
          pubkey,
        }),
      {
        channelName: CHANNEL,
        parentEventId: ROOT_ID,
        pubkey: TEST_IDENTITIES.alice.pubkey,
      },
    );

    const summary = page.locator(
      `[data-testid="message-thread-summary"][data-thread-head-id="${ROOT_ID}"]`,
    );
    await expect(summary).toBeVisible();
    await expectNoHighlight(page);
    await page.mouse.move(0, 0);
    await waitForAnimations(page);
    await page.screenshot({ path: `${SHOTS}/${theme}-before.png` });

    await summary.click();
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
    const marked = page.locator('[data-open-thread-head="true"]');
    await expect(marked).toHaveCount(1);
    await expect(marked).toHaveAttribute("aria-current", "true");
    await expect(marked.locator(`[data-message-id="${ROOT_ID}"]`)).toHaveCount(
      1,
    );
    await expect(
      page.locator('[data-testid="message-timeline"] [aria-current]'),
    ).toHaveCount(1);
    await page.mouse.move(0, 0);
    await waitForAnimations(page);
    await page.screenshot({ path: `${SHOTS}/${theme}-after.png` });
    await expectStableBackground(marked);

    await page.getByTestId("auxiliary-panel-close").click();
    await expect(page.getByTestId("message-thread-panel")).toHaveCount(0);
    await expectNoHighlight(page);
  });

  test(`${theme}: highlights an open root without replies`, async ({
    page,
  }) => {
    await openChannel(page, theme);
    await expectNoHighlight(page);
    const root = page.locator(
      `[data-testid="message-row"][data-message-id="${ROOT_ID}"]`,
    );
    await root.hover();
    await root.getByRole("button", { name: "Reply" }).click();
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
    const marked = page.locator('[data-open-thread-head="true"]');
    await expect(marked).toHaveCount(1);
    await expect(marked).toHaveAttribute("aria-current", "true");
    await expect(marked.locator(`[data-message-id="${ROOT_ID}"]`)).toHaveCount(
      1,
    );
    await expect(
      page.locator('[data-testid="message-timeline"] [aria-current]'),
    ).toHaveCount(1);
    await expectStableBackground(marked);
    await page.getByTestId("auxiliary-panel-close").click();
    await expect(page.getByTestId("message-thread-panel")).toHaveCount(0);
    await expectNoHighlight(page);
  });
}
