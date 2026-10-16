import assert from "node:assert/strict";
import { after, afterEach, before, test } from "node:test";

import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

before(() => {
  Object.assign(globalThis, {
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
    window: dom.window,
  });
});

afterEach(async () => {
  const { cleanup } = await import("@testing-library/react");
  cleanup();
  document.body.replaceChildren();
});

after(() => dom.window.close());

async function mountAutofocus(focus, draftKey = "huddle") {
  const React = await import("react");
  const { render } = await import("@testing-library/react");
  const { useComposerAutofocus } = await import("./useComposerAutofocus.ts");
  function Harness(props) {
    useComposerAutofocus(props.focus, props.draftKey, false);
    return null;
  }
  const view = render(React.createElement(Harness, { focus, draftKey }));
  return (nextFocus, nextKey = draftKey) =>
    view.rerender(
      React.createElement(Harness, { focus: nextFocus, draftKey: nextKey }),
    );
}

function editorInput() {
  const editor = document.createElement("input");
  document.body.append(editor);
  return editor;
}

test("late composer readiness preserves focus on a dialog switch", async () => {
  const editor = editorInput();
  const dialog = document.createElement("div");
  dialog.setAttribute("role", "dialog");
  const control = document.createElement("button");
  control.setAttribute("role", "switch");
  dialog.append(control);
  document.body.append(dialog);

  const rerender = await mountAutofocus(() => {});
  control.focus();
  rerender(() => editor.focus());
  assert.equal(document.activeElement, control);
});

test("a draft change preserves focus on an open dialog button", async () => {
  const editor = editorInput();
  const dialog = document.createElement("div");
  dialog.setAttribute("role", "dialog");
  const control = document.createElement("button");
  dialog.append(control);
  document.body.append(dialog);

  const focus = () => editor.focus();
  const rerender = await mountAutofocus(focus, "before");
  control.focus();
  rerender(focus, "huddle");
  assert.equal(document.activeElement, control);
});

test("initial mount and navigation from an ordinary button still focus the editor", async () => {
  const editor = editorInput();
  const navigation = document.createElement("button");
  document.body.append(navigation);
  const focus = () => editor.focus();
  const rerender = await mountAutofocus(focus, "before");
  assert.equal(document.activeElement, editor);

  navigation.focus();
  rerender(focus, "huddle");
  assert.equal(document.activeElement, editor);
});
