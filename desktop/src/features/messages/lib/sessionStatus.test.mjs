import assert from "node:assert/strict";
import test from "node:test";

import {
  CONTEXT_HIGHLIGHT_PERCENT,
  describeHistory,
  describeSessionTransition,
  formatContextPercent,
  isHighContext,
  parseSessionInfoTag,
  parseSessionPrimaryTag,
  parseSessionStatusTags,
} from "./sessionStatus.ts";

const UUID_A = "6f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b";

function primary(state = "new", reason = "first_in_thread", percent = "") {
  return ["buzz-session", state, reason, percent];
}

function info({
  version = "1",
  transition = UUID_A,
  scope = "conversation",
  read = "",
  total = "",
  mode = "unknown",
} = {}) {
  return ["buzz-session-info", version, transition, scope, read, total, mode];
}

// ---------------------------------------------------------------------------
// Primary tag
// ---------------------------------------------------------------------------

test("primary tag: known states parse with absent percent", () => {
  for (const state of ["new", "resumed", "compacted", "restarted"]) {
    assert.deepEqual(parseSessionPrimaryTag(primary(state)), {
      state,
      reason: "first_in_thread",
      percent: null,
    });
  }
});

test("primary tag: empty percent is absent, never 0", () => {
  const parsed = parseSessionPrimaryTag(primary("new", "first_in_thread", ""));
  assert.equal(parsed?.percent, null);
});

test("primary tag: percent boundaries 0, 79, 80, 100 parse as integers", () => {
  for (const value of [0, 79, 80, 100]) {
    assert.equal(
      parseSessionPrimaryTag(primary("resumed", "idle_sleep", String(value)))
        ?.percent,
      value,
    );
  }
});

test("primary tag: malformed numbers make the tag invalid", () => {
  for (const bad of [
    "-1",
    "101",
    "080",
    "50.5",
    " 50",
    "50%",
    "≈50",
    "1e2",
    "NaN",
    "0x10",
    "١٠",
  ]) {
    assert.equal(
      parseSessionPrimaryTag(primary("new", "first_in_thread", bad)),
      null,
      `percent ${JSON.stringify(bad)} must be rejected`,
    );
  }
});

test("primary tag: unknown state, wrong arity or non-string fields are invalid", () => {
  assert.equal(parseSessionPrimaryTag(primary("sleeping")), null);
  assert.equal(parseSessionPrimaryTag(primary("NEW")), null);
  assert.equal(
    parseSessionPrimaryTag(["buzz-session", "new", "first_in_thread"]),
    null,
  );
  assert.equal(
    parseSessionPrimaryTag([...primary("new", "first_in_thread", "5"), "x"]),
    null,
  );
  assert.equal(parseSessionPrimaryTag(["buzz-session", "new", 5, "10"]), null);
  assert.equal(parseSessionPrimaryTag(["other", "new", "x", ""]), null);
  assert.equal(parseSessionPrimaryTag(undefined), null);
});

test("primary tag: hostile or oversized reasons make the tag invalid", () => {
  for (const reason of [
    "",
    "x".repeat(97),
    "resume_failed:/home/user/.secret/session.json",
    "resume_failed:Connection refused",
    "<script>alert(1)</script>",
    "idle sleep",
    "idle_sleep\n",
    "IDLE_SLEEP",
    "résumé",
  ]) {
    assert.equal(
      parseSessionPrimaryTag(primary("new", reason)),
      null,
      `reason ${JSON.stringify(reason)} must be rejected`,
    );
  }
  // 96 bytes is the wire limit and stays valid (neutral copy, see below).
  assert.equal(
    parseSessionPrimaryTag(primary("new", "x".repeat(96)))?.reason,
    "x".repeat(96),
  );
});

// ---------------------------------------------------------------------------
// Supporting tag
// ---------------------------------------------------------------------------

test("info tag: exact counts parse", () => {
  assert.deepEqual(
    parseSessionInfoTag(
      info({ scope: "thread", read: "12", total: "37", mode: "exact" }),
    ),
    {
      transitionId: UUID_A,
      scope: "thread",
      read: 12,
      total: 37,
      mode: "exact",
    },
  );
});

test("info tag: lower bound may omit read or total", () => {
  assert.deepEqual(parseSessionInfoTag(info({ mode: "lower_bound" })), {
    transitionId: UUID_A,
    scope: "conversation",
    read: null,
    total: null,
    mode: "lower_bound",
  });
  assert.equal(
    parseSessionInfoTag(info({ total: "40", mode: "lower_bound" }))?.total,
    40,
  );
});

test("info tag: malformed version, UUID, scope, counts or mode are rejected", () => {
  const bad = [
    info({ version: "2" }),
    info({ version: "" }),
    info({ transition: "not-a-uuid" }),
    info({ transition: UUID_A.toUpperCase() }),
    info({ transition: `{${UUID_A}}` }),
    info({ transition: UUID_A.replaceAll("-", "") }),
    info({ scope: "channel" }),
    info({ read: "-1", mode: "unknown" }),
    info({ read: "01", mode: "unknown" }),
    info({ read: "1.5", mode: "unknown" }),
    info({ read: "99999999999999999999", mode: "unknown" }),
    info({ read: "5", total: "4", mode: "lower_bound" }),
    info({ read: "5", total: "", mode: "exact" }),
    info({ read: "", total: "5", mode: "exact" }),
    info({ read: "5", total: "9", mode: "unknown" }),
    info({ mode: "approximate" }),
    ["buzz-session-info", "1", UUID_A, "thread", "", ""],
    [...info(), "extra"],
  ];
  for (const tag of bad) {
    assert.equal(
      parseSessionInfoTag(tag),
      null,
      `info ${JSON.stringify(tag)} must be rejected`,
    );
  }
});

// ---------------------------------------------------------------------------
// Tag set
// ---------------------------------------------------------------------------

test("tag set: no primary tag means no status", () => {
  assert.equal(parseSessionStatusTags([["h", "c"], info()]), null);
  assert.equal(parseSessionStatusTags(undefined), null);
});

test("tag set: valid primary with invalid supporting metadata keeps no info", () => {
  const parsed = parseSessionStatusTags([
    primary("new", "first_in_thread", "12"),
    info({ version: "9", read: "3", total: "4", mode: "exact" }),
  ]);
  assert.equal(parsed?.primary.percent, 12);
  assert.equal(parsed?.info, null);
});

test("tag set: duplicate identical primary tags are accepted", () => {
  const parsed = parseSessionStatusTags([
    primary("resumed", "idle_sleep", "40"),
    primary("resumed", "idle_sleep", "40"),
    info(),
  ]);
  assert.equal(parsed?.primary.state, "resumed");
  assert.equal(parsed?.info?.transitionId, UUID_A);
});

test("tag set: conflicting primary tags yield no status", () => {
  assert.equal(
    parseSessionStatusTags([
      primary("resumed", "idle_sleep", "40"),
      primary("new", "first_in_thread", "40"),
    ]),
    null,
  );
  assert.equal(
    parseSessionStatusTags([
      primary("resumed", "idle_sleep", "40"),
      primary("resumed", "idle_sleep", "41"),
    ]),
    null,
  );
});

test("tag set: one invalid primary among duplicates yields no status", () => {
  assert.equal(
    parseSessionStatusTags([
      primary("resumed", "idle_sleep", "40"),
      primary("resumed", "idle_sleep", "4000"),
    ]),
    null,
  );
});

test("tag set: conflicting supporting tags drop the info, not the primary", () => {
  const parsed = parseSessionStatusTags([
    primary("new", "first_in_thread", ""),
    info(),
    info({ transition: "0f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b" }),
  ]);
  assert.equal(parsed?.primary.state, "new");
  assert.equal(parsed?.info, null);
});

// ---------------------------------------------------------------------------
// Copy
// ---------------------------------------------------------------------------

function copy(state, reason) {
  return describeSessionTransition({ state, reason, percent: null });
}

test("copy: known new/resumed reasons", () => {
  assert.deepEqual(copy("new", "first_in_thread"), { title: "New session" });
  assert.deepEqual(copy("resumed", "idle_sleep"), {
    title: "Resumed after sleep",
  });
  assert.deepEqual(copy("resumed", "harness_restart"), {
    title: "Resumed after restart",
  });
  assert.deepEqual(copy("resumed", "worker_crash"), {
    title: "Resumed after worker failure",
  });
  assert.deepEqual(copy("new", "discarded"), {
    title: "New session",
    detail: "previous session was discarded",
  });
  assert.deepEqual(copy("new", "config_changed"), {
    title: "New session",
    detail: "agent configuration changed",
  });
  assert.deepEqual(copy("new", "live_elsewhere"), {
    title: "New session",
    detail: "previous session is active elsewhere",
  });
  assert.deepEqual(copy("new", "resume_unsupported"), {
    title: "New session",
    detail: "agent cannot resume sessions",
  });
  assert.deepEqual(copy("new", "context_overflow"), {
    title: "New session",
    detail: "context window was full",
  });
  assert.deepEqual(copy("new", "max_turns"), {
    title: "New session",
    detail: "turn limit reached",
  });
});

test("copy: resume failure codes get fixed wording, never the raw code", () => {
  const expected = {
    not_persisted: "resume failed: previous session was not saved",
    store_unreadable: "resume failed: saved session could not be read",
    store_busy: "resume failed: session store was busy",
    not_found: "resume failed: previous session not found",
    timeout: "resume failed: timed out",
    protocol: "resume failed: protocol error",
    "rpc_-32603": "resume failed: agent error",
    rpc_42: "resume failed: agent error",
    unknown: "resume failed",
    some_future_code: "resume failed",
  };
  for (const [code, detail] of Object.entries(expected)) {
    assert.deepEqual(copy("new", `resume_failed:${code}`), {
      title: "New session",
      detail,
    });
  }
});

test("copy: unknown but well-formed reasons get neutral copy without the raw string", () => {
  const reason = "user_restart:recent";
  for (const [state, title] of [
    ["new", "New session"],
    ["resumed", "Resumed session"],
  ]) {
    const result = copy(state, reason);
    assert.deepEqual(result, { title });
    assert.ok(!JSON.stringify(result).includes("user_restart"));
  }
  // A known resume reason paired with an unexpected state stays neutral.
  assert.deepEqual(copy("resumed", "first_in_thread"), {
    title: "Resumed session",
  });
  assert.deepEqual(copy("new", "idle_sleep"), { title: "New session" });
});

test("copy: unmatched compacted/restarted reasons keep neutral copy", () => {
  for (const state of ["compacted", "restarted"]) {
    for (const reason of ["future_reason", "idle_sleep"]) {
      assert.deepEqual(copy(state, reason), { title: "Session updated" });
    }
  }
});

function history(fields) {
  return describeHistory({
    transitionId: UUID_A,
    scope: "thread",
    read: null,
    total: null,
    mode: "unknown",
    ...fields,
  });
}

test("history: exact count", () => {
  assert.deepEqual(history({ read: 12, total: 37, mode: "exact" }), [
    "read 12 of 37 messages",
  ]);
});

test("history: proven lower bound", () => {
  assert.deepEqual(history({ read: 12, total: 37, mode: "lower_bound" }), [
    "read 12 of at least 37 messages",
  ]);
});

test("history: incomplete without a total says earlier history may be omitted", () => {
  assert.deepEqual(history({ read: 12, total: null, mode: "lower_bound" }), [
    "read 12 messages",
    "Earlier history may be omitted",
  ]);
  assert.deepEqual(history({ read: null, total: null, mode: "lower_bound" }), [
    "Earlier history may be omitted",
  ]);
  assert.deepEqual(history({ read: null, total: 37, mode: "lower_bound" }), [
    "Earlier history may be omitted",
  ]);
});

test("history: unknown completeness makes no ratio or omission claim", () => {
  assert.deepEqual(history({ read: 12, mode: "unknown" }), [
    "read 12 messages",
  ]);
  assert.deepEqual(history({ read: null, mode: "unknown" }), []);
});

test("history: one message reads naturally", () => {
  assert.deepEqual(history({ read: 1, total: 1, mode: "exact" }), [
    "read 1 of 1 message",
  ]);
  assert.deepEqual(history({ read: 1, mode: "unknown" }), ["read 1 message"]);
});

test("percent: plain value without approximation sign, highlighted from 80%", () => {
  assert.equal(CONTEXT_HIGHLIGHT_PERCENT, 80);
  assert.equal(formatContextPercent(0), "0% context");
  assert.equal(formatContextPercent(79), "79% context");
  assert.equal(formatContextPercent(80), "80% context (high)");
  assert.equal(formatContextPercent(100), "100% context (high)");
  assert.equal(isHighContext(79), false);
  assert.equal(isHighContext(80), true);
  for (const value of [0, 79, 80, 100]) {
    assert.ok(!formatContextPercent(value).includes("≈"));
  }
});

test("copy: bounded compact and restart reasons describe the actual transition", () => {
  assert.deepEqual(copy("compacted", "user_compact"), {
    title: "Context compacted",
  });
  assert.deepEqual(copy("compacted", "auto_compact"), {
    title: "Context compacted automatically",
  });
  assert.deepEqual(copy("resumed", "worker_recycled"), {
    title: "Resumed after worker restart",
  });
  for (const depth of ["none", "recent", "all"])
    assert.equal(
      copy("restarted", `user_restart:${depth}`).title,
      "Session started over",
    );
});
