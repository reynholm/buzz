import assert from "node:assert/strict";
import { test, afterEach } from "node:test";
import { createElement as h } from "react";
import {
  render,
  cleanup,
  fireEvent,
  act,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  CommunitiesProvider,
  useCommunities,
} from "../../communities/useCommunities.tsx";
import { TooltipProvider } from "../../../shared/ui/tooltip.tsx";
import { ThreadContextControls } from "./ThreadContextControls.tsx";
import { useThreadReplies } from "../useThreadReplies.ts";
import { threadRepliesKey } from "../lib/messageQueryKeys.ts";
const root = "1".repeat(64),
  signer = "b".repeat(64),
  agent = "a".repeat(64);
const channel = {
  id: "channel",
  channelType: "stream",
  isMember: true,
  archivedAt: null,
};
const head = {
  id: root,
  signerPubkey: signer,
  pubkey: signer,
  kind: 9,
  tags: [
    ["h", channel.id],
    ["p", agent],
  ],
  body: "Root",
  createdAt: 1,
};
const bot = {
  pubkey: agent,
  name: "Scout",
  ownerPubkey: signer,
  respondTo: "owner-only",
  respondToAllowlist: [],
  channelIds: [channel.id],
};
const clients = [];
const OriginalImage = window.Image;
afterEach(() => {
  cleanup();
  for (const client of clients.splice(0)) client.clear();
  delete window.__TAURI_INTERNALS__;
  localStorage.clear();
  window.Image = OriginalImage;
});
function setup({
  pending = false,
  historyFails = false,
  denied = false,
  headOverride = head,
  directory = denied ? [] : [bot],
  expectedTarget = null,
  historyFetcher = null,
  ownerEnabled = true,
  activeChannel = channel,
  ownerChannel = activeChannel,
  profiles = {},
} = {}) {
  localStorage.setItem(
    "buzz-communities",
    JSON.stringify([
      {
        id: "test",
        name: "Test",
        relayUrl: "wss://example.test",
        pubkey: signer,
      },
    ]),
  );
  localStorage.setItem("buzz-active-community-id", "test");
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  });
  clients.push(client);
  client.setQueryData(["identity"], { pubkey: signer });
  client.setQueryData(["relay-agents"], directory);
  client.setQueryData(["channels", channel.id, "detail"], activeChannel);
  const sends = [];
  const profileRequests = [];
  let resolveHistory;
  window.__TAURI_INTERNALS__ = {
    invoke: async (command, payload) => {
      if (command === "readd_community_relay") return;
      if (command === "get_users_batch") {
        profileRequests.push(payload.pubkeys);
        return {
          profiles,
          missing: payload.pubkeys.filter((pubkey) => !profiles[pubkey]),
        };
      }
      if (command === "get_thread_replies") {
        if (historyFetcher) return historyFetcher();
        if (historyFails) throw new Error("History unavailable");
        return pending
          ? new Promise((resolve) => {
              resolveHistory = resolve;
            })
          : { events: [], next_cursor: null };
      }
      if (command === "send_channel_message") {
        sends.push(payload);
        return "sent";
      }
      throw new Error(`Unexpected IPC ${command}`);
    },
  };
  let community;
  let owner;
  function Harness({ nextHead }) {
    community = useCommunities();
    const query = useThreadReplies(
      ownerEnabled ? ownerChannel : null,
      nextHead ? (nextHead.rootId ?? nextHead.id) : null,
      expectedTarget,
    );
    owner = {
      data: query.data,
      isPending: query.isPending,
      isError: query.isError,
    };
    return h(
      TooltipProvider,
      null,
      h(ThreadContextControls, {
        channel: activeChannel,
        threadHead: nextHead,
      }),
    );
  }
  const tree = (nextHead) =>
    h(
      QueryClientProvider,
      { client },
      h(CommunitiesProvider, null, h(Harness, { nextHead })),
    );
  const view = render(tree(headOverride));
  const open = async () => {
    await act(async () => {
      fireEvent.keyDown(view.getByRole("button", { name: "Manage context" }), {
        key: "Enter",
      });
    });
  };
  return {
    client,
    view,
    sends,
    profileRequests,
    open,
    owner: () => owner,
    rerender: (nextHead) => view.rerender(tree(nextHead)),
    allowHistory: () => {
      historyFails = false;
    },
    switchRelay: () =>
      community.updateCommunity("test", { relayUrl: "wss://other.test" }),
    resolveHistory: async () => {
      await act(async () => resolveHistory({ events: [], next_cursor: null }));
    },
  };
}
test("production picker always lists one agent first, selection sends nothing and Back restores the roster", async () => {
  const { view, open, sends } = setup();
  await open();
  const select = await view.findByRole("menuitem", { name: "Scout" });
  await act(async () => fireEvent.click(select));
  assert.ok(view.getByRole("menuitem", { name: "Compact context" }));
  assert.equal(sends.length, 0);
  await act(async () =>
    fireEvent.click(view.getByRole("menuitem", { name: "Back to agents" })),
  );
  assert.ok(view.getByRole("menuitem", { name: "Scout" }));
  assert.ok(
    !view.queryByRole("menuitem", { name: "Compact context" }),
    "Back removes selected actions",
  );
});
test("production roster resolves profile images and falls back without duplicating accessible labels", async () => {
  window.Image = class extends window.EventTarget {
    complete = true;
    naturalWidth = 1;
    src = "";
  };
  const second = { ...bot, pubkey: "c".repeat(64), name: "Backup" };
  const { view, open, profileRequests, sends } = setup({
    directory: [bot, second],
    headOverride: { ...head, tags: [...head.tags, ["p", second.pubkey]] },
    profiles: {
      [agent]: {
        display_name: "Profile display name",
        avatar_url: "https://example.test/scout.png",
      },
    },
  });
  await open();
  const scout = await view.findByRole("menuitem", {
    name: "Scout",
    exact: true,
  });
  const backup = view.getByRole("menuitem", { name: "Backup", exact: true });
  const image = await view.findByTestId("thread-context-agent-avatar-image");
  assert.equal(image.getAttribute("src"), "https://example.test/scout.png");
  assert.ok(scout.contains(image));
  for (const row of [scout, backup]) {
    const avatar = row.querySelector(
      '[data-testid="thread-context-agent-avatar"]',
    );
    assert.ok(avatar);
    assert.equal(avatar.getAttribute("data-avatar-shape"), "squircle");
    assert.ok(avatar.classList.contains("h-5"));
    assert.equal(avatar.parentElement.getAttribute("aria-hidden"), "true");
  }
  await waitFor(() =>
    assert.equal(
      backup.querySelector(
        '[data-testid="thread-context-agent-avatar-fallback"]',
      )?.textContent,
      "B",
    ),
  );
  assert.equal(
    view.queryAllByRole("img").length,
    0,
    "Avatars are decorative beside the menu labels",
  );
  assert.deepEqual(profileRequests, [[agent, second.pubkey]]);
  assert.equal(sends.length, 0);
});
test("production picker waits for complete history and recovers without exposing actions", async () => {
  const { view, open, resolveHistory } = setup({ pending: true });
  await open();
  assert.match(view.getByRole("status").textContent, /Loading thread agents/);
  assert.ok(
    !view.queryByRole("menuitem"),
    "Incomplete history exposes no actions",
  );
  await resolveHistory();
  assert.ok(await view.findByRole("menuitem", { name: "Scout" }));
});
test("denied roster exposes only a neutral empty state", async () => {
  const { view, open } = setup({ denied: true });
  await open();
  assert.match(
    (await view.findByRole("status")).textContent,
    /No agents you can manage/,
  );
  assert.ok(
    !view.queryByRole("menuitem"),
    "Denied agents expose no action or identity",
  );
});
test("failed history offers a recoverable roster without executable agent actions", async () => {
  const { view, open, sends, allowHistory } = setup({ historyFails: true });
  await open();
  // The production owner exhausts its ordinary 1s/2s/4s retry backoff first.
  const retry = await view.findByRole(
    "menuitem",
    { name: "Retry" },
    { timeout: 10_000 },
  );
  assert.match(
    view.getByRole("status").textContent,
    /Could not load thread agents/,
  );
  assert.ok(
    !view.queryByRole("menuitem", { name: "Scout" }),
    "Failed history cannot reveal a partial roster",
  );
  assert.ok(
    !view.queryByRole("menuitem", { name: "Compact context" }),
    "Failed history offers no executable context action",
  );
  allowHistory();
  await act(async () => fireEvent.click(retry));
  assert.ok(await view.findByRole("menuitem", { name: "Scout" }));
  assert.equal(sends.length, 0);
});

for (const channelType of ["stream", "forum"]) {
  test(`opening the picker preserves the owning notification query validation across gated retries (${channelType})`, async () => {
    const expected = "d".repeat(64);
    const reply = {
      id: expected,
      pubkey: agent,
      kind: 9,
      tags: [
        ["h", channel.id],
        ["e", root, "", "reply"],
      ],
      created_at: 2,
      content: "The notification reply has replicated.",
      sig: "",
    };
    let calls = 0;
    let available = false;
    let releaseRetry;
    const retryGate = new Promise((resolve) => {
      releaseRetry = resolve;
    });
    const { view, client, open, owner } = setup({
      activeChannel: { ...channel, channelType },
      ownerChannel: channel,
      expectedTarget: expected,
      historyFetcher: async () => {
        calls += 1;
        if (calls === 2) await retryGate;
        return { events: available ? [reply] : [], next_cursor: null };
      },
    });
    const state = () =>
      client.getQueryState(threadRepliesKey(channel.id, root));
    await waitFor(() => assert.equal(state()?.fetchFailureCount, 1));
    await open();
    assert.ok(
      owner().isPending,
      "The owner is still validating its missing notification reply",
    );
    assert.ok(
      !view.queryByRole("menuitem", { name: "Scout" }),
      "The picker cannot claim a complete roster during owner backoff",
    );
    await waitFor(() => assert.equal(calls, 2), { timeout: 2_000 });
    await act(async () => {
      releaseRetry();
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    assert.equal(state()?.fetchFailureCount, 2);
    assert.equal(state()?.status, "pending");
    assert.ok(
      owner().isPending,
      "A second missing page set must not resolve the owner's validation",
    );
    assert.ok(
      !view.queryByRole("menuitem", { name: "Scout" }),
      "The picker must remain unavailable after a missing-target retry",
    );
    available = true;
    assert.ok(
      await view.findByRole("menuitem", { name: "Scout" }, { timeout: 3_000 }),
    );
    assert.equal(calls, 3);
    assert.equal(owner().isPending, false);
    assert.ok(
      owner().data.some((event) => event.id === expected),
      "The owning bounded query recovers the actual notification reply",
    );
  });
}

test("a missing non-forum owning query is unavailable rather than a complete roster", async () => {
  const { view, client, open, sends } = setup({ ownerEnabled: false });
  await open();
  await act(async () =>
    client.setQueryData(threadRepliesKey(channel.id, root), []),
  );
  assert.match(
    view.getByRole("status").textContent,
    /Thread context is unavailable here/,
  );
  assert.ok(
    !view.queryByRole("menuitem"),
    "A disabled owner cannot expose cached or partial agents",
  );
  await act(async () =>
    client.removeQueries({
      queryKey: threadRepliesKey(channel.id, root),
      exact: true,
    }),
  );
  assert.match(
    view.getByRole("status").textContent,
    /Thread context is unavailable here/,
  );
  assert.ok(
    !view.queryByRole("menuitem"),
    "A missing owner cannot expose agents",
  );
  assert.equal(sends.length, 0);
});

test("read-only forum previews retain a bounded roster without replacing the disabled owner", async () => {
  const second = { ...bot, pubkey: "c".repeat(64), name: "Surveyor" };
  let calls = 0;
  const { view, client, open, owner, sends } = setup({
    activeChannel: { ...channel, channelType: "forum" },
    headOverride: { ...head, kind: 40002 },
    directory: [bot, second],
    historyFetcher: async () => {
      calls += 1;
      return calls === 1
        ? {
            events: [],
            next_cursor: { created_at: 2, event_id: "d".repeat(64) },
          }
        : {
            events: [
              {
                id: "e".repeat(64),
                pubkey: second.pubkey,
                kind: 9,
                tags: [
                  ["h", channel.id],
                  ["e", root, "", "reply"],
                ],
                created_at: 3,
                content: "Later-page participant",
                sig: "",
              },
            ],
            next_cursor: null,
          };
    },
  });
  const shared = client
    .getQueryCache()
    .find({ queryKey: threadRepliesKey(channel.id, root), exact: true });
  const originalFn = shared.options.queryFn;
  await open();
  assert.ok(await view.findByRole("menuitem", { name: "Surveyor" }));
  assert.equal(calls, 2);
  assert.ok(
    owner().isPending,
    "The disabled forum owner was not fetched or rewritten",
  );
  assert.equal(
    shared.options.queryFn === originalFn,
    true,
    "The fallback cannot replace the owner query function",
  );
  assert.equal(shared.state.data, undefined);
  assert.equal(sends.length, 0);
});
test("revocation and thread or requester switches reset the selected action surface", async () => {
  const { view, client, open, rerender } = setup();
  await open();
  await view.findByRole("menuitem", { name: "Scout" });
  await act(async () =>
    fireEvent.click(view.getByRole("menuitem", { name: "Scout" })),
  );
  await act(async () => client.setQueryData(["relay-agents"], []));
  // Query cache notifications are scheduled; wait for the production subscriber.
  // Assert absence as a boolean so a failure never inspects the live DOM graph.
  await waitFor(() =>
    assert.ok(
      !view.queryByRole("menuitem", { name: "Compact context" }),
      "Revocation removes selected actions",
    ),
  );
  await act(async () => client.setQueryData(["relay-agents"], [bot]));
  await view.findByRole("menuitem", { name: "Scout" });
  await act(async () =>
    fireEvent.click(view.getByRole("menuitem", { name: "Scout" })),
  );
  rerender({ ...head, id: "2".repeat(64) });
  assert.ok(
    !view.queryByRole("menuitem", { name: "Compact context" }),
    "Thread changes remove selected actions",
  );
  await open();
  await view.findByRole("menuitem", { name: "Scout" });
  await act(async () =>
    fireEvent.click(view.getByRole("menuitem", { name: "Scout" })),
  );
  await act(async () =>
    client.setQueryData(["identity"], { pubkey: "c".repeat(64) }),
  );
  await waitFor(() =>
    assert.ok(
      !view.queryByRole("menuitem", { name: "Compact context" }),
      "Requester changes remove selected actions",
    ),
  );
});
test("empty and optimistic thread heads have no context control", () => {
  const { view, rerender } = setup({ headOverride: null });
  assert.ok(
    !view.queryByRole("button"),
    "An empty thread has no context control",
  );
  rerender({ ...head, pending: true });
  assert.ok(
    !view.queryByRole("button"),
    "An optimistic head has no context control",
  );
});

test("same-name agents remain distinct and a relay switch closes selected actions", async () => {
  const second = { ...bot, pubkey: "c".repeat(64) };
  const { view, open, sends, switchRelay } = setup({
    directory: [bot, second, bot],
    headOverride: { ...head, tags: [...head.tags, ["p", second.pubkey]] },
  });
  await open();
  const entries = await view.findAllByRole("menuitem");
  assert.equal(entries.length, 2);
  assert.notEqual(entries[0].textContent, entries[1].textContent);
  await act(async () => fireEvent.click(entries[1]));
  assert.ok(view.getByRole("menuitem", { name: "Compact context" }));
  assert.equal(sends.length, 0);
  await act(async () => switchRelay());
  assert.ok(
    !view.queryByRole("menuitem"),
    "A relay switch closes the previous scope",
  );
  assert.equal(sends.length, 0);
});
