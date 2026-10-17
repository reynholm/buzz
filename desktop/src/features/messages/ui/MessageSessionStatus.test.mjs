import assert from "node:assert/strict";
import { after, afterEach, before, test } from "node:test";

import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

before(() => {
  Object.assign(globalThis, {
    document: dom.window.document,
    Element: dom.window.Element,
    getComputedStyle: dom.window.getComputedStyle,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
    MutationObserver: dom.window.MutationObserver,
    Node: dom.window.Node,
    SVGElement: dom.window.SVGElement,
    window: dom.window,
  });
  dom.window.matchMedia = () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  });
});

afterEach(async () => {
  const { cleanup } = await import("@testing-library/react");
  cleanup();
});

after(() => dom.window.close());

function normalizeText(text) {
  return text.replace(/\s+/g, " ").trim();
}

async function renderStatus(status) {
  const { createElement } = await import("react");
  const { render } = await import("@testing-library/react");
  const { MessageSessionStatus } = await import("./MessageSessionStatus.tsx");
  const { container } = render(createElement(MessageSessionStatus, { status }));
  return container;
}

function visibleText(row) {
  // Screen-reader-only separators are not part of the visible line.
  const clone = row.cloneNode(true);
  for (const node of clone.querySelectorAll(".sr-only")) node.remove();
  return normalizeText(clone.textContent);
}

test("explanation, history and percentage render as one muted line", async () => {
  const container = await renderStatus({
    explanation: {
      title: "New session",
      detail: "resume failed: previous session not found",
      history: ["read 12 of 37 messages"],
    },
    percent: 64,
  });
  const row = container.querySelector('[data-testid="message-session-status"]');
  assert.ok(row);
  assert.equal(
    visibleText(row),
    "New session · resume failed: previous session not found · read 12 of 37 messages · 64% context",
  );
  assert.match(row.className, /text-xs/);
  assert.match(row.className, /text-muted-foreground/);
  const percent = row.querySelector(
    '[data-testid="message-session-status-percent"]',
  );
  assert.equal(percent.getAttribute("data-level"), "normal");
  assert.doesNotMatch(percent.className, /text-warning/);
});

test("percentage alone renders without an explanation", async () => {
  const container = await renderStatus({ percent: 79 });
  const row = container.querySelector('[data-testid="message-session-status"]');
  assert.equal(visibleText(row), "79% context");
});

test("80% and above is emphasized with readable text, not color alone", async () => {
  for (const value of [80, 100]) {
    const container = await renderStatus({ percent: value });
    const percent = container.querySelector(
      '[data-testid="message-session-status-percent"]',
    );
    assert.equal(percent.getAttribute("data-level"), "high");
    assert.match(percent.className, /text-warning/);
    assert.match(percent.className, /font-medium/);
    assert.equal(
      normalizeText(percent.textContent),
      `${value}% context (high)`,
    );
    const { cleanup } = await import("@testing-library/react");
    cleanup();
  }
});

test("row is noninteractive: no tab stop, live region or extra label", async () => {
  const container = await renderStatus({
    explanation: { title: "Resumed after sleep" },
    percent: 81,
  });
  const row = container.querySelector('[data-testid="message-session-status"]');
  for (const element of [row, ...row.querySelectorAll("*")]) {
    assert.equal(element.hasAttribute("tabindex"), false);
    assert.equal(element.hasAttribute("aria-live"), false);
    assert.equal(element.hasAttribute("aria-label"), false);
    assert.equal(element.hasAttribute("role"), false);
    assert.ok(!["BUTTON", "A", "INPUT"].includes(element.tagName));
  }
  // Separators are hidden from assistive technology and replaced by a
  // pause, so the line is read once as a list of phrases.
  for (const separator of row.querySelectorAll('[aria-hidden="true"]')) {
    assert.equal(separator.textContent.trim(), "·");
  }
  assert.equal(
    normalizeText(
      [...row.childNodes]
        .filter((node) => node.getAttribute?.("aria-hidden") !== "true")
        .map((node) => node.textContent)
        .join(""),
    ),
    "Resumed after sleep, 81% context (high)",
  );
});

test("MessageRow memo rerenders when only the derived status moves", async () => {
  const { MessageRow } = await import("./MessageRow.tsx");
  const base = {
    id: "m1",
    createdAt: 1,
    author: "Agent",
    body: "hi",
    depth: 0,
    time: "",
    isAgent: true,
    tags: [["buzz-session", "new", "first_in_thread", "40"]],
  };
  const owner = {
    ...base,
    sessionStatus: { explanation: { title: "New session" }, percent: 40 },
  };
  const demoted = { ...base, sessionStatus: { percent: 40 } };
  assert.equal(typeof MessageRow.compare, "function");
  assert.equal(
    MessageRow.compare({ message: owner }, { message: demoted }),
    false,
    "losing the explanation must rerender the row even with identical tags",
  );
  assert.equal(
    MessageRow.compare(
      { message: owner },
      {
        message: {
          ...base,
          sessionStatus: {
            explanation: { title: "New session" },
            percent: 40,
          },
        },
      },
    ),
    true,
    "a fresh but equal status keeps the memo",
  );
});
