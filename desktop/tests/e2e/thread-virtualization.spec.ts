import { expect, test, type Page } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

async function setup(page: Page) {
  await installMockBridge(page);
  await page.goto("/");
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );
}

test("long threads bound mounted replies and preserve navigation and live scroll ownership", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await setup(page);
  const { rootId, replyIds } = await page.evaluate(() => {
    const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
    if (!emit) throw new Error("Mock emitter unavailable");
    const now = Math.floor(Date.now() / 1_000);
    const root = emit({
      channelName: "engineering",
      content: "Long thread",
      createdAt: now - 700,
    });
    const replyIds = Array.from(
      { length: 600 },
      (_, index) =>
        emit({
          channelName: "engineering",
          parentEventId: root.id,
          createdAt: now - 699 + index,
          content: `Reply ${index}\n\n**Details** with a [reference](https://example.com).\n\n- first item\n- second item\n\n\`\`\`ts\nconst value = ${index};\n\`\`\``,
        }).id,
    );
    return { rootId: root.id, replyIds };
  });
  await page.getByTestId("channel-engineering").click();
  await page
    .locator(
      `[data-testid="message-thread-summary"][data-thread-head-id="${rootId}"]`,
    )
    .click();
  const body = page.getByTestId("message-thread-body");
  const replies = page.getByTestId("message-thread-replies");
  const row = (id: string) => body.locator(`[data-message-id="${id}"]`);
  await expect(row(replyIds[599])).toBeInViewport();
  await expect
    .poll(() => replies.locator("[data-message-id]").count())
    .toBeLessThan(100);
  await expect
    .poll(() =>
      body.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop),
    )
    .toBeLessThan(4);

  await body.hover();
  await page.mouse.wheel(0, -3_000);
  await expect(page.getByTestId("thread-scroll-to-latest")).toBeVisible();
  await expect
    .poll(() => replies.locator("[data-message-id]").count())
    .toBeLessThan(100);
  await body.focus();
  await page.keyboard.press("Home");
  await expect(row(replyIds[0])).toBeInViewport();

  // The router receives the same target fields as a native buzz://message link.
  const channelPath = new URL(page.url()).hash.split("?")[0];
  await page.evaluate(
    ({ path, rootId, id }) => {
      location.hash = `${path}?messageId=${id}&threadRootId=${rootId}`;
    },
    { path: channelPath, rootId, id: replyIds[300] },
  );
  await expect(row(replyIds[300])).toBeInViewport();
  const before = await row(replyIds[300]).evaluate(
    (el) => el.getBoundingClientRect().top,
  );
  await page.evaluate((rootId) => {
    window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "engineering",
      parentEventId: rootId,
      content: "Arrived while reading history",
      createdAt: Math.floor(Date.now() / 1_000),
    });
  }, rootId);
  await expect
    .poll(async () =>
      Math.abs(
        (await row(replyIds[300]).evaluate(
          (el) => el.getBoundingClientRect().top,
        )) - before,
      ),
    )
    .toBeLessThan(4);
  await page.getByTestId("thread-scroll-to-latest").click();
  await expect(
    body.getByText("Arrived while reading history", { exact: true }),
  ).toBeInViewport();
  await expect
    .poll(() =>
      body.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop),
    )
    .toBeLessThan(4);
  const last = body.getByText("Arrived while reading history", { exact: true });
  const composer = page.getByTestId("thread-composer-overlay");
  expect(
    await last.evaluate((el) => el.getBoundingClientRect().bottom),
  ).toBeLessThanOrEqual(
    (await composer.evaluate((el) => el.getBoundingClientRect().top)) + 2,
  );
});

test("a channel with many thread roots keeps its mounted history bounded", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await setup(page);
  await page.evaluate(() => {
    const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
    if (!emit) throw new Error("Mock emitter unavailable");
    const now = Math.floor(Date.now() / 1_000);
    for (let index = 0; index < 1_200; index += 1) {
      const root = emit({
        channelName: "engineering",
        content: `Topic ${index}`,
        createdAt: now - 2_400 + index * 2,
      });
      emit({
        channelName: "engineering",
        content: `Answer ${index}`,
        parentEventId: root.id,
        createdAt: now - 2_399 + index * 2,
      });
    }
  });
  await page.getByTestId("channel-engineering").click();
  const timeline = page.getByTestId("message-timeline");
  await expect(
    timeline.getByText("Topic 1199", { exact: true }),
  ).toBeInViewport();
  await expect
    .poll(() => timeline.locator("[data-message-id]").count())
    .toBeLessThan(300);
  await timeline.hover();
  await page.mouse.wheel(0, -2_000);
  await expect
    .poll(() => timeline.locator("[data-message-id]").count())
    .toBeLessThan(300);
});
