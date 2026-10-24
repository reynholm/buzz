import { expect, test, type Page } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { waitForAnimations } from "../helpers/animations";

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
  await expect(page.getByTestId("thread-scroll-to-latest")).toHaveText(
    "1 new message",
  );
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

test("a live reply in an earlier expanded branch preserves the reading row", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await setup(page);
  const { rootId, branchId, flat } = await page.evaluate(() => {
    const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
    if (!emit) throw new Error("Mock emitter unavailable");
    const now = Math.floor(Date.now() / 1_000);
    const root = emit({
      channelName: "engineering",
      content: "Branch insertion thread",
      createdAt: now - 1_100,
    });
    const branch = emit({
      channelName: "engineering",
      content: "Earlier expanded branch",
      parentEventId: root.id,
      createdAt: now - 1_000,
    });
    emit({
      channelName: "engineering",
      content: "Initial child",
      parentEventId: branch.id,
      createdAt: now - 999,
    });
    const flat = Array.from(
      { length: 400 },
      (_, index) =>
        emit({
          channelName: "engineering",
          parentEventId: root.id,
          content: `Reading reply ${index}`,
          createdAt: now - 900 + index,
        }).id,
    );
    return { rootId: root.id, branchId: branch.id, flat };
  });
  await page.getByTestId("channel-engineering").click();
  await page
    .locator(
      `[data-testid="message-thread-summary"][data-thread-head-id="${rootId}"]`,
    )
    .click();
  const body = page.getByTestId("message-thread-body");
  await expect(
    body.locator(`[data-message-id="${flat[399]}"]`),
  ).toBeInViewport();
  await body.evaluate(async (el) => {
    let previous = "";
    let stable = 0;
    for (let frame = 0; frame < 180; frame += 1) {
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => resolve()),
      );
      const geometry = `${el.scrollTop}:${el.scrollHeight}:${el.clientHeight}`;
      const atBottom = el.scrollHeight - el.clientHeight - el.scrollTop < 4;
      stable = atBottom && geometry === previous ? stable + 1 : 0;
      if (stable >= 6) return;
      previous = geometry;
    }
    throw new Error("Initial thread measurements did not settle");
  });
  await waitForAnimations(page);
  await body.focus();
  await page.keyboard.press("Home");
  await page
    .locator(
      `[data-testid="message-thread-summary"][data-thread-head-id="${branchId}"]`,
    )
    .click();
  await expect(body.getByText("Initial child", { exact: true })).toBeVisible();
  await body.focus();
  await page.keyboard.press("End");
  await expect
    .poll(() =>
      body.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop),
    )
    .toBeLessThan(4);
  await expect(page.getByTestId("thread-scroll-to-latest")).toHaveCount(0);
  await body.evaluate((el) => {
    el.dispatchEvent(new WheelEvent("wheel", { bubbles: true, deltaY: 3_000 }));
    el.scrollTop = el.scrollHeight * 0.55;
  });
  const findReadingRow = () =>
    body.evaluate((el) => {
      const top = el.getBoundingClientRect().top;
      const row = Array.from(
        el.querySelectorAll<HTMLElement>("[data-message-id]"),
      ).find((row) => {
        const r = row.getBoundingClientRect();
        return r.top >= top && r.bottom < top + el.clientHeight - 150;
      });
      return row?.dataset.messageId
        ? { id: row.dataset.messageId, top: row.getBoundingClientRect().top }
        : null;
    });
  await expect.poll(findReadingRow).not.toBeNull();
  const reading = await findReadingRow();
  if (!reading) throw new Error("Missing reading row");
  await page.evaluate(
    (branchId) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "engineering",
        parentEventId: branchId,
        content: `Inserted above the reader\n\n${"Long nested details. ".repeat(60)}`,
        createdAt: Math.floor(Date.now() / 1_000) + 1,
      }),
    branchId,
  );
  const latest = page.getByTestId("thread-scroll-to-latest");
  await expect(latest).toHaveText("1 new message");
  // The arrival count proves the deferred thread list committed the insertion.
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  await expect
    .poll(() =>
      body
        .locator(`[data-message-id="${reading.id}"]`)
        .evaluate(
          (row, top) => Math.abs(row.getBoundingClientRect().top - top),
          reading.top,
        ),
    )
    .toBeLessThan(4);
  await body.focus();
  await page.keyboard.press("End");
  await expect(latest).toHaveCount(0);
});

test("End releases a short thread target even without physical scroll movement", async ({
  page,
}) => {
  await setup(page);
  const { rootId, replyId } = await page.evaluate(() => {
    const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
    if (!emit) throw new Error("Mock emitter unavailable");
    const root = emit({
      channelName: "engineering",
      content: "Short target root",
    });
    const reply = emit({
      channelName: "engineering",
      parentEventId: root.id,
      content: "Short target reply",
    });
    return { rootId: root.id, replyId: reply.id };
  });
  await page.getByTestId("channel-engineering").click();
  const path = new URL(page.url()).hash.split("?")[0];
  await page.evaluate(
    ({ path, rootId, replyId }) => {
      location.hash = `${path}?messageId=${replyId}&threadRootId=${rootId}`;
    },
    { path, rootId, replyId },
  );
  const body = page.getByTestId("message-thread-body");
  await expect(body.locator(`[data-message-id="${replyId}"]`)).toBeInViewport();
  const latest = page.getByTestId("thread-scroll-to-latest");
  await expect(latest).toBeVisible();
  await expect
    .poll(() =>
      body.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop),
    )
    .toBeLessThan(4);
  const offset = await body.evaluate((el) => el.scrollTop);
  await body.focus();
  await page.keyboard.press("End");
  await expect(latest).toHaveCount(0);
  expect(await body.evaluate((el) => el.scrollTop)).toBe(offset);
  await page.evaluate((rootId) => {
    window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "engineering",
      parentEventId: rootId,
      content: `Live reply after End\n\n${"More details. ".repeat(100)}`,
      createdAt: Math.floor(Date.now() / 1_000) + 1,
    });
  }, rootId);
  await expect(latest).toHaveCount(0);
  await expect
    .poll(() =>
      body.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop),
    )
    .toBeLessThan(4);
});
