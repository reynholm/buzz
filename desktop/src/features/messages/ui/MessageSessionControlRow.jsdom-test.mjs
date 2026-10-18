import assert from "node:assert/strict";
import { test, afterEach } from "node:test";
import { createElement } from "react";
import { render, cleanup } from "@testing-library/react";
import { MessageSessionControlRow } from "./MessageSessionControlRow.tsx";
import { MessageRow } from "./MessageRow.tsx";
afterEach(cleanup);
const control = {
  action: "Compact context",
  targets: [
    {
      pubkey: "a".repeat(64),
      name: "Agent A",
      state: "success",
      detail: "Completed",
    },
  ],
};
test("compact row renders actor, action, target and independent outcome without raw command text", () => {
  const view = render(
    createElement(MessageSessionControlRow, {
      actor: "Human",
      control,
      createdAt: Date.now() / 1000,
    }),
  );
  assert.equal(
    view.container.textContent,
    "Human: Compact context · Agent A · Completed",
  );
  assert.equal(view.container.querySelectorAll("button").length, 0);
  assert.doesNotMatch(view.container.textContent, /!compact|raw\/path/);
});
test("a request without a signed result expires while a signed pending result keeps its explanation", () => {
  const target = {
    ...control.targets[0],
    state: "unconfirmed",
    detail: "Result not confirmed",
  };
  const view = render(
    createElement(MessageSessionControlRow, {
      actor: "Human",
      control: { ...control, targets: [target] },
      createdAt: 1,
    }),
  );
  assert.match(
    view.container.textContent,
    /Request expired · Result not confirmed/,
  );
  const pending = render(
    createElement(MessageSessionControlRow, {
      actor: "Human",
      control: {
        ...control,
        targets: [
          {
            ...target,
            state: "pending",
            detail: "Waiting for current work to stop",
          },
        ],
      },
      createdAt: 1,
    }),
  );
  assert.match(
    pending.container.textContent,
    /Waiting for current work to stop/,
  );
  assert.doesNotMatch(pending.container.textContent, /expired/);
});
test("MessageRow memo invalidates when only a raw correlated result changes", () => {
  const message = {
    id: "m",
    createdAt: 1,
    pubkey: "h",
    body: "!compact",
    author: "Human",
    depth: 0,
    time: "",
    sessionControl: control,
  };
  assert.equal(
    MessageRow.compare(
      { message },
      {
        message: {
          ...message,
          sessionControl: {
            ...control,
            targets: [
              {
                ...control.targets[0],
                state: "failure",
                detail: "Compaction failed",
              },
            ],
          },
        },
      },
    ),
    false,
  );
  assert.equal(
    MessageRow.compare(
      { message },
      { message: { ...message, sessionControl: structuredClone(control) } },
    ),
    true,
  );
});
