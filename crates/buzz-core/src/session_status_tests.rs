use super::*;
use serde_json::json;

fn agent() -> String {
    nostr::Keys::generate().public_key().to_hex()
}

fn root() -> String {
    "ab".repeat(32)
}

fn status() -> SessionStatus {
    SessionStatus {
        state: SessionState::Resumed,
        reason: StatusReason::IdleSleep,
        transition_id: Uuid::new_v4(),
        occupancy: ContextOccupancy::new(64, 100),
        history: None,
    }
}

fn envelope() -> StatusEnvelope {
    StatusEnvelope::new(
        canonical_identity("ws://relay.example", &agent()).unwrap(),
        Uuid::new_v4(),
        StatusScope {
            channel_id: Uuid::new_v4(),
            thread_root: Some(root()),
        },
        Uuid::new_v4(),
        status(),
    )
}

// -- occupancy ---------------------------------------------------------

#[test]
fn percent_table() {
    let cases: &[(u64, u64, Option<u8>)] = &[
        (64, 100, Some(64)),
        (80, 100, Some(80)),
        (0, 100, Some(0)),
        (79, 100, Some(79)),
        (1, 3, Some(33)),
        (2, 3, Some(66)),
        (5, 0, None),
        (0, 0, None),
        (101, 100, Some(100)),
        (u64::MAX, 1, Some(100)),
        (u64::MAX, u64::MAX, Some(100)),
        (u64::MAX - 1, u64::MAX, Some(99)),
        (u64::MAX / 2, u64::MAX, Some(49)),
    ];
    for &(used, size, expected) in cases {
        let occupancy = ContextOccupancy { used, size };
        assert_eq!(occupancy.percent(), expected, "used={used} size={size}");
    }
}

#[test]
fn occupancy_constructor_rejects_zero_size() {
    assert_eq!(ContextOccupancy::new(1, 0), None);
    assert_eq!(
        ContextOccupancy::new(1, 2),
        Some(ContextOccupancy { used: 1, size: 2 })
    );
}

#[test]
fn occupancy_from_json_rejects_unusable_inputs() {
    let ok = ContextOccupancy::from_json(Some(&json!(64)), Some(&json!(100)));
    assert_eq!(ok, ContextOccupancy::new(64, 100));
    let integral_float = ContextOccupancy::from_json(Some(&json!(64.0)), Some(&json!(100.0)));
    assert_eq!(integral_float, ContextOccupancy::new(64, 100));
    let bad: &[(Option<serde_json::Value>, Option<serde_json::Value>)] = &[
        (None, Some(json!(100))),
        (Some(json!(1)), None),
        (Some(json!(-1)), Some(json!(100))),
        (Some(json!(1)), Some(json!(-100))),
        (Some(json!(1.5)), Some(json!(100))),
        (Some(json!(1)), Some(json!(100.25))),
        (Some(json!("64")), Some(json!(100))),
        (Some(json!(null)), Some(json!(100))),
        (Some(json!(1)), Some(json!(0))),
        (Some(json!(1e30)), Some(json!(100))),
    ];
    for (used, size) in bad {
        assert_eq!(
            ContextOccupancy::from_json(used.as_ref(), size.as_ref()),
            None,
            "used={used:?} size={size:?}"
        );
    }
}

// -- tags --------------------------------------------------------------

#[test]
fn primary_tag_carries_percent_and_empty_when_unavailable() {
    let scope = StatusScope {
        channel_id: Uuid::new_v4(),
        thread_root: None,
    };
    let mut s = status();
    let tags = s.event_tags(&scope).unwrap();
    assert_eq!(tags[0], ["buzz-session", "resumed", "idle_sleep", "64"]);

    s.occupancy = ContextOccupancy::new(0, 100);
    assert_eq!(s.event_tags(&scope).unwrap()[0][3], "0");

    s.occupancy = None;
    let tags = s.event_tags(&scope).unwrap();
    assert_eq!(tags[0], ["buzz-session", "resumed", "idle_sleep", ""]);

    // A directly constructed zero-size occupancy is rejected, never shown as 0%.
    s.occupancy = Some(ContextOccupancy { used: 3, size: 0 });
    assert_eq!(s.event_tags(&scope), Err(StatusError::InvalidOccupancy));
}

#[test]
fn info_tag_encodes_each_history_mode() {
    let thread = StatusScope {
        channel_id: Uuid::new_v4(),
        thread_root: Some(root()),
    };
    let conversation = StatusScope {
        channel_id: thread.channel_id,
        thread_root: None,
    };
    let mut s = status();
    let id = s.transition_id.to_string();
    let info = |s: &SessionStatus, scope: &StatusScope| s.event_tags(scope).unwrap()[1].clone();

    s.history = Some(HistoryCoverage {
        read: Some(12),
        total: Some(12),
        exact: true,
        incomplete: false,
    });
    assert_eq!(
        info(&s, &thread),
        ["buzz-session-info", "1", &id, "thread", "12", "12", "exact"]
    );

    s.history = Some(HistoryCoverage {
        read: Some(12),
        total: Some(40),
        exact: false,
        incomplete: false,
    });
    assert_eq!(
        info(&s, &conversation),
        [
            "buzz-session-info",
            "1",
            &id,
            "conversation",
            "12",
            "40",
            "lower_bound"
        ]
    );

    // Known incomplete, total unavailable: lower_bound with empty total.
    s.history = Some(HistoryCoverage {
        read: Some(12),
        total: None,
        exact: false,
        incomplete: true,
    });
    assert_eq!(
        info(&s, &thread),
        [
            "buzz-session-info",
            "1",
            &id,
            "thread",
            "12",
            "",
            "lower_bound"
        ]
    );

    // Known incomplete and no event identities: still lower_bound.
    s.history = Some(HistoryCoverage {
        read: None,
        total: None,
        exact: false,
        incomplete: true,
    });
    assert_eq!(
        info(&s, &thread),
        [
            "buzz-session-info",
            "1",
            &id,
            "thread",
            "",
            "",
            "lower_bound"
        ]
    );

    // Incomplete with a total: the total is a lower bound too.
    s.history = Some(HistoryCoverage {
        read: Some(12),
        total: Some(30),
        exact: false,
        incomplete: true,
    });
    assert_eq!(
        info(&s, &thread),
        [
            "buzz-session-info",
            "1",
            &id,
            "thread",
            "12",
            "30",
            "lower_bound"
        ]
    );

    // Missing event identities with a proven lower-bound total.
    s.history = Some(HistoryCoverage {
        read: None,
        total: Some(7),
        exact: false,
        incomplete: false,
    });
    assert_eq!(
        info(&s, &thread),
        [
            "buzz-session-info",
            "1",
            &id,
            "thread",
            "",
            "7",
            "lower_bound"
        ]
    );

    // No completeness claim: no total, not incomplete, not exact.
    s.history = Some(HistoryCoverage {
        read: Some(12),
        total: None,
        exact: false,
        incomplete: false,
    });
    assert_eq!(
        info(&s, &thread),
        ["buzz-session-info", "1", &id, "thread", "12", "", "unknown"]
    );
    s.history = Some(HistoryCoverage {
        read: None,
        total: None,
        exact: false,
        incomplete: false,
    });
    assert_eq!(
        info(&s, &thread),
        ["buzz-session-info", "1", &id, "thread", "", "", "unknown"]
    );

    s.history = None;
    assert_eq!(
        info(&s, &thread),
        ["buzz-session-info", "1", &id, "thread", "", "", "unknown"]
    );
    assert_eq!(SESSION_INFO_VERSION, "1");
}

#[test]
fn inconsistent_history_is_rejected() {
    let bad = [
        HistoryCoverage {
            read: Some(5),
            total: Some(4),
            exact: false,
            incomplete: false,
        },
        HistoryCoverage {
            read: Some(5),
            total: Some(5),
            exact: true,
            incomplete: true,
        },
        HistoryCoverage {
            read: Some(5),
            total: None,
            exact: true,
            incomplete: false,
        },
        HistoryCoverage {
            read: None,
            total: Some(5),
            exact: true,
            incomplete: false,
        },
    ];
    let scope = StatusScope {
        channel_id: Uuid::new_v4(),
        thread_root: None,
    };
    for history in bad {
        let mut s = status();
        s.history = Some(history);
        assert_eq!(
            s.event_tags(&scope),
            Err(StatusError::InvalidHistory),
            "{history:?}"
        );
    }
}

#[test]
fn tags_leak_no_identity_path_or_provider_id() {
    let private = "/home/someone/private-dir";
    let relay = format!("ws://relay.example/{}?token={private}", "base");
    let agent = agent();
    let env = StatusEnvelope::new(
        canonical_identity(&relay, &agent).unwrap(),
        Uuid::new_v4(),
        StatusScope {
            channel_id: Uuid::new_v4(),
            thread_root: Some(root()),
        },
        Uuid::new_v4(),
        status(),
    );
    let text = env.event_tags().unwrap().concat().join("|");
    for secret in [
        private,
        "private-dir",
        "relay.example",
        &agent,
        &env.scope.channel_id.to_string(),
        &env.scope.thread_root.clone().unwrap(),
        &env.instance_id.to_string(),
        &env.generation.to_string(),
    ] {
        assert!(!text.contains(secret), "tags leaked {secret}");
    }
    // A path smuggled through the reason code is rejected, not emitted.
    let mut s = status();
    s.reason = StatusReason::ResumeFailed(format!("..{private}"));
    assert_eq!(s.event_tags(&env.scope), Err(StatusError::InvalidReason));
}

// -- reasons -----------------------------------------------------------

#[test]
fn reason_vocabulary_round_trips() {
    for wire in [
        "first_in_thread",
        "idle_sleep",
        "harness_restart",
        "worker_crash",
        "discarded",
        "live_elsewhere",
        "resume_unsupported",
        "config_changed",
        "context_overflow",
        "max_turns",
        "resume_failed:not_found",
        "resume_failed:rpc_-32603",
        "resume_failed:timeout",
        "resume_failed:protocol",
    ] {
        let reason = StatusReason::from_wire(wire).expect(wire);
        assert_eq!(reason.as_wire(), wire);
    }
}

#[test]
fn unknown_or_unsafe_reasons_are_rejected() {
    let long = format!("resume_failed:{}", "a".repeat(83));
    for wire in [
        "",
        "bogus",
        "First_In_Thread",
        "resume_failed:",
        "resume_failed:Has Space",
        "resume_failed:a/b",
        "resume_failed:C0DE",
        "resume_failed:caf\u{e9}",
        "resume_failed:line\nbreak",
        long.as_str(),
    ] {
        assert_eq!(
            StatusReason::from_wire(wire),
            Err(StatusError::InvalidReason),
            "{wire:?}"
        );
    }
    let longest = format!("resume_failed:{}", "a".repeat(82));
    assert_eq!(longest.len(), MAX_REASON_BYTES);
    assert!(StatusReason::from_wire(&longest).is_ok());
}

#[test]
fn state_vocabulary_is_closed_over_four_values() {
    let states = [
        (SessionState::New, "new"),
        (SessionState::Resumed, "resumed"),
        (SessionState::Compacted, "compacted"),
        (SessionState::Restarted, "restarted"),
    ];
    for (state, wire) in states {
        assert_eq!(state.as_wire(), wire);
        assert_eq!(serde_json::to_value(state).unwrap(), json!(wire));
        assert_eq!(
            serde_json::from_value::<SessionState>(json!(wire)).unwrap(),
            state
        );
    }
    assert!(serde_json::from_value::<SessionState>(json!("paused")).is_err());
}

// -- identity ----------------------------------------------------------

#[test]
fn identity_matches_same_spelling_and_transport_counterparts() {
    let a = agent();
    let base = canonical_identity("wss://relay.example", &a).unwrap();
    for same in [
        "wss://relay.example",
        "wss://relay.example/",
        "WSS://Relay.Example:443/",
        "https://relay.example",
        "https://relay.example/",
        "  https://RELAY.example:443  ",
    ] {
        assert_eq!(canonical_identity(same, &a).unwrap(), base, "{same}");
    }
    let ws = canonical_identity("ws://relay.example", &a).unwrap();
    assert_eq!(
        canonical_identity("http://relay.example:80/", &a).unwrap(),
        ws
    );
    assert_ne!(ws, base, "plain and secure transports stay distinct");
    assert_eq!(base.relay, "wss://relay.example");
    assert_eq!(base.agent, a);
    // Idempotent.
    assert_eq!(canonical_identity(&base.relay, &base.agent).unwrap(), base);
    // Uppercase key hex normalizes.
    assert_eq!(
        canonical_identity("wss://relay.example", &a.to_uppercase()).unwrap(),
        base
    );
}

#[test]
fn identity_differs_on_host_agent_port_path_and_query() {
    let a = agent();
    let id = |relay: &str, agent: &str| canonical_identity(relay, agent).unwrap();
    let base = id("ws://relay.example", &a);
    for other in [
        "ws://other.example",
        "ws://relay.example:8080",
        "ws://relay.example/tenant",
        "ws://relay.example/tenant/",
        "ws://relay.example/?community=a",
        "wss://relay.example",
    ] {
        assert_ne!(id(other, &a), base, "{other}");
    }
    assert_ne!(
        id("ws://relay.example/tenant", &a),
        id("ws://relay.example/tenant/", &a)
    );
    assert_ne!(
        id("ws://relay.example/?community=a", &a),
        id("ws://relay.example/?community=b", &a)
    );
    assert_ne!(
        id("ws://relay.example", &a),
        id("ws://relay.example", &agent())
    );

    // Each loopback spelling is its own host.
    let spellings = ["ws://localhost", "ws://127.0.0.1", "ws://[::1]"];
    for (i, x) in spellings.iter().enumerate() {
        for y in &spellings[i + 1..] {
            assert_ne!(id(x, &a), id(y, &a), "{x} vs {y}");
        }
    }
    assert_eq!(id("ws://LOCALHOST:80", &a), id("ws://localhost", &a));
}

#[test]
fn invalid_identity_inputs_are_rejected() {
    let a = agent();
    for relay in [
        "",
        "not a url",
        "ftp://relay.example",
        "file:///tmp/x",
        "ws://",
        "wss://user@relay.example",
        "wss://user:pw@relay.example",
        "wss://relay.example/#frag",
        "mailto:x@y.z",
    ] {
        assert_eq!(
            canonical_identity(relay, &a),
            Err(StatusError::InvalidRelay),
            "{relay:?}"
        );
    }
    for key in ["", "xyz", "npub1abc", &"0".repeat(63), &"g".repeat(64)] {
        assert_eq!(
            canonical_identity("ws://relay.example", key),
            Err(StatusError::InvalidAgent),
            "{key:?}"
        );
    }
}

// -- scope and names ---------------------------------------------------

#[test]
fn scope_file_names_are_distinct_and_path_safe() {
    let channel = Uuid::new_v4();
    let conversation = StatusScope {
        channel_id: channel,
        thread_root: None,
    };
    let thread = StatusScope {
        channel_id: channel,
        thread_root: Some(root()),
    };
    let other_root = StatusScope {
        channel_id: channel,
        thread_root: Some("cd".repeat(32)),
    };
    let names: Vec<String> = [&conversation, &thread, &other_root]
        .iter()
        .map(|s| scope_file_name(s).unwrap())
        .collect();
    assert_eq!(names[0], format!("{channel}.conversation.json"));
    assert_eq!(names[1], format!("{channel}.thread-{}.json", root()));
    assert_ne!(names[1], names[2]);
    for name in &names {
        assert!(name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'));
        assert!(!name.contains(".."));
    }
}

#[test]
fn invalid_thread_roots_never_become_file_names() {
    for root in [
        "",
        "ab",
        "../../etc/passwd",
        &"AB".repeat(32),
        &"zz".repeat(32),
        &format!("{}/", "a".repeat(63)),
        &"a".repeat(65),
    ] {
        let scope = StatusScope {
            channel_id: Uuid::new_v4(),
            thread_root: Some(root.to_owned()),
        };
        assert_eq!(
            scope_file_name(&scope),
            Err(StatusError::InvalidThreadRoot),
            "{root:?}"
        );
    }
}

#[test]
fn instance_layout_names() {
    let generation = Uuid::new_v4();
    assert_eq!(INSTANCE_LOCK_FILE, "instance.lock");
    assert_eq!(
        generation_lock_path(generation),
        format!("generations/{generation}.lock")
    );
}

// -- snapshot ----------------------------------------------------------

#[test]
fn snapshot_round_trips() {
    let mut env = envelope();
    env.status.history = Some(HistoryCoverage {
        read: Some(3),
        total: Some(9),
        exact: false,
        incomplete: false,
    });
    let bytes = env.to_snapshot_bytes().unwrap();
    assert!(bytes.len() <= MAX_SNAPSHOT_BYTES);
    assert_eq!(parse_snapshot(&bytes).unwrap(), env);
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["status"]["reason"], "idle_sleep");
    assert_eq!(value["status"]["state"], "resumed");
}

#[test]
fn snapshot_rejects_unknown_versions_and_bad_shapes() {
    let good: serde_json::Value =
        serde_json::from_slice(&envelope().to_snapshot_bytes().unwrap()).unwrap();
    let parse = |v: &serde_json::Value| parse_snapshot(&serde_json::to_vec(v).unwrap());

    for version in [json!(0), json!(3), json!(255), json!(1000)] {
        let mut v = good.clone();
        v["version"] = version;
        assert_eq!(parse(&v), Err(StatusError::UnsupportedVersion));
    }
    let mut v = good.clone();
    v.as_object_mut().unwrap().remove("version");
    assert_eq!(parse(&v), Err(StatusError::Malformed));

    let mut v = good.clone();
    v["extra"] = json!(1);
    assert_eq!(parse(&v), Err(StatusError::Malformed));

    let mut v = good.clone();
    v["status"]["state"] = json!("paused");
    assert_eq!(parse(&v), Err(StatusError::Malformed));

    let mut v = good.clone();
    v["status"]["reason"] = json!("resume_failed:/etc/passwd");
    assert_eq!(parse(&v), Err(StatusError::Malformed));

    let mut v = good.clone();
    v["scope"]["thread_root"] = json!("../x");
    assert_eq!(parse(&v), Err(StatusError::InvalidThreadRoot));

    let mut v = good.clone();
    v["status"]["occupancy"] = json!({"used": 1, "size": 0});
    assert_eq!(parse(&v), Err(StatusError::InvalidOccupancy));

    let mut v = good.clone();
    v["status"]["history"] = json!({"read": 9, "total": 3, "exact": false, "incomplete": false});
    assert_eq!(parse(&v), Err(StatusError::InvalidHistory));

    // Non-canonical identity (uppercase scheme/host, http scheme).
    let mut v = good.clone();
    v["identity"]["relay"] = json!("http://relay.example");
    assert_eq!(parse(&v), Err(StatusError::InvalidRelay));

    assert_eq!(parse_snapshot(b"not json"), Err(StatusError::Malformed));
    assert_eq!(parse_snapshot(b""), Err(StatusError::Malformed));
    assert_eq!(parse_snapshot(b"[1]"), Err(StatusError::Malformed));
}

#[test]
fn snapshot_size_limit_is_enforced_on_both_sides() {
    let env = envelope();
    let bytes = env.to_snapshot_bytes().unwrap();

    // Pad inside valid JSON so only the size limit can reject it.
    let mut padded = bytes.clone();
    padded.truncate(padded.len() - 1);
    padded.extend_from_slice(b",\"pad\":\"");
    padded.resize(MAX_SNAPSHOT_BYTES + 1, b'x');
    assert_eq!(parse_snapshot(&padded), Err(StatusError::TooLarge));
    assert_eq!(
        parse_snapshot(&vec![b' '; MAX_SNAPSHOT_BYTES + 1]),
        Err(StatusError::TooLarge)
    );

    // Exactly at the limit (trailing whitespace) is still accepted.
    let mut at_limit = bytes;
    at_limit.resize(MAX_SNAPSHOT_BYTES, b' ');
    assert_eq!(parse_snapshot(&at_limit), Ok(env));

    // The writer side refuses oversized documents too.
    let mut huge = envelope();
    huge.identity.relay = format!("ws://relay.example/{}", "p".repeat(MAX_SNAPSHOT_BYTES));
    assert_eq!(huge.to_snapshot_bytes(), Err(StatusError::TooLarge));
}
