use buzz_core::session_control::{
    ControlAction, ControlFailure, ControlOrigin, ControlResult, RestartDepth, SessionCommand,
    SessionResult,
};
use buzz_core::session_status::{
    canonical_identity, parse_snapshot, SessionState, SessionStatus, StatusEnvelope, StatusReason,
    StatusScope,
};
use serde_json::json;
use uuid::Uuid;

const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ACTOR: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const AGENT: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

#[test]
fn command_grammar_is_exact() {
    for (text, command) in [
        ("!compact", SessionCommand::Compact),
        ("!restart", SessionCommand::Restart(RestartDepth::Recent)),
        ("!restart none", SessionCommand::Restart(RestartDepth::None)),
        ("!restart all", SessionCommand::Restart(RestartDepth::All)),
        ("!rotate", SessionCommand::Restart(RestartDepth::Recent)),
    ] {
        assert_eq!(SessionCommand::parse(9, text), Some(command));
        assert_eq!(
            SessionCommand::parse(9, &format!(" \n{text}\t")),
            Some(command)
        );
        assert_eq!(SessionCommand::parse(1, text), None);
        assert_eq!(SessionCommand::parse(7, text), None);
        assert_eq!(
            SessionCommand::parse(9, command.as_content()),
            Some(command)
        );
    }
    for text in [
        "!restart recent",
        "!restart foo",
        "@Name !restart",
        "> !restart",
        "say !compact",
        "!restart  all",
        "!restart\nall",
        "!COMPACT",
        "!compact later",
    ] {
        assert_eq!(SessionCommand::parse(9, text), None, "{text}");
    }
}

#[test]
fn result_tags_are_strict_and_pending_has_terminal_precedence() {
    let pending = ControlResult::Pending;
    let ok = ControlResult::Terminal(Ok(()));
    let error = ControlResult::Terminal(Err(ControlFailure::Busy));
    for value in [pending, ok, error] {
        let result = SessionResult {
            command_id: ID.into(),
            result: value,
        };
        let tag = result.event_tag().unwrap();
        assert_eq!(tag.len(), 5);
        assert_eq!(
            SessionResult::from_tags(value.reaction(), &[tag.clone(), tag.clone()]),
            Some(result)
        );
        let mut malformed = tag.clone();
        malformed[2] = ID.to_uppercase();
        assert_eq!(
            SessionResult::from_tags(value.reaction(), &[malformed]),
            None
        );
        let mut malformed = tag;
        malformed[4] = "raw/path/error".into();
        assert_eq!(
            SessionResult::from_tags(value.reaction(), &[malformed]),
            None
        );
    }
    let good = SessionResult {
        command_id: ID.into(),
        result: ok,
    }
    .event_tag()
    .unwrap();
    let bad = SessionResult {
        command_id: ID.into(),
        result: error,
    }
    .event_tag()
    .unwrap();
    assert_eq!(SessionResult::from_tags("✅", &[good.clone(), bad]), None);
    assert_eq!(SessionResult::from_tags("❌", &[good]), None);
    assert_eq!(pending.merge(ok), ok);
    assert_eq!(ok.merge(pending), ok);
    assert_eq!(ok.merge(ok), ok);
    assert_eq!(
        ok.merge(error),
        ControlResult::Terminal(Err(ControlFailure::Unconfirmed))
    );
    assert_eq!(
        error.merge(ok),
        ControlResult::Terminal(Err(ControlFailure::Unconfirmed))
    );
}

fn envelope() -> StatusEnvelope {
    StatusEnvelope::new(
        canonical_identity("wss://relay.example", AGENT).unwrap(),
        Uuid::from_u128(1),
        StatusScope {
            channel_id: Uuid::from_u128(2),
            thread_root: None,
        },
        Uuid::from_u128(3),
        SessionStatus {
            state: SessionState::Restarted,
            reason: StatusReason::UserRestart(RestartDepth::All),
            transition_id: Uuid::from_u128(4),
            occupancy: None,
            history: None,
        },
    )
}

#[test]
fn snapshot_legacy_v1_and_v2_actor_preserve_wire_dimensions() {
    let mut snapshot = envelope();
    let mut legacy = serde_json::to_value(&snapshot).unwrap();
    legacy["version"] = json!(1);
    legacy.as_object_mut().unwrap().remove("origin");
    assert!(parse_snapshot(&serde_json::to_vec(&legacy).unwrap()).is_ok());
    legacy["origin"] = json!(null);
    assert!(parse_snapshot(&serde_json::to_vec(&legacy).unwrap()).is_err());
    snapshot.origin = Some(ControlOrigin {
        command_event_id: ID.into(),
        actor_pubkey: ACTOR.into(),
    });
    let bytes = snapshot.to_snapshot_bytes().unwrap();
    assert_eq!(parse_snapshot(&bytes).unwrap(), snapshot);
    let tags = snapshot.event_tags().unwrap();
    assert_eq!(tags[0].len(), 4);
    assert_eq!(tags[1].len(), 7);
    assert_eq!(tags[2].len(), 5);
    let action = ControlAction::from_tags(&tags).unwrap();
    assert_eq!(action.origin, snapshot.origin.clone().unwrap());
    assert_eq!(action.transition_id, snapshot.status.transition_id);
    let mut future = serde_json::to_value(snapshot).unwrap();
    future["version"] = json!(3);
    assert!(parse_snapshot(&serde_json::to_vec(&future).unwrap()).is_err());
}

#[test]
fn action_tags_require_matching_valid_info_and_no_conflicts() {
    let mut snapshot = envelope();
    snapshot.origin = Some(ControlOrigin {
        command_event_id: ID.into(),
        actor_pubkey: ACTOR.into(),
    });
    let tags = snapshot.event_tags().unwrap();
    assert_eq!(ControlAction::from_tags(&tags[2..]), None);
    for field in [2, 3, 4] {
        let mut broken = tags.clone();
        broken[2][field] = "malformed".into();
        assert_eq!(ControlAction::from_tags(&broken), None);
    }
    let mut conflict = tags.clone();
    let mut other = tags[2].clone();
    other[4] = ID.into();
    conflict.push(other);
    assert_eq!(ControlAction::from_tags(&conflict), None);
    let mut duplicate = tags.clone();
    duplicate.push(tags[2].clone());
    assert!(ControlAction::from_tags(&duplicate).is_some());
    let mut auto = tags;
    auto[0][2] = "auto_compact".into();
    assert_eq!(ControlAction::from_tags(&auto), None);
}

#[test]
fn shared_reason_and_origin_fixture_corpus() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/session_controls.json")).unwrap();
    for case in cases["reasons"].as_array().unwrap() {
        assert_eq!(
            StatusReason::from_wire(case["wire"].as_str().unwrap()).is_ok(),
            case["valid"].as_bool().unwrap()
        );
    }
    for case in cases["origins"].as_array().unwrap() {
        let origin: ControlOrigin = serde_json::from_value(case["origin"].clone()).unwrap();
        assert_eq!(origin.validate().is_ok(), case["valid"].as_bool().unwrap());
    }
    for case in cases["precedence"].as_array().unwrap() {
        let a: ControlResult = serde_json::from_value(case["first"].clone()).unwrap();
        let b: ControlResult = serde_json::from_value(case["second"].clone()).unwrap();
        assert_eq!(serde_json::to_value(a.merge(b)).unwrap(), case["expected"]);
    }
}

#[test]
fn freshness_boundaries_and_failure_vocabulary_are_closed() {
    use buzz_core::session_control::{control_is_fresh, ControlOutcome};
    assert!(control_is_fresh(1000, 1900));
    assert!(!control_is_fresh(1000, 1901));
    assert!(control_is_fresh(1030, 1000));
    assert!(!control_is_fresh(1031, 1000));
    assert!(control_is_fresh(u64::MAX, u64::MAX));
    for code in [
        "no_session",
        "unsupported",
        "scope_unavailable",
        "busy",
        "stale_target",
        "stale_request",
        "store_unavailable",
        "resume_failed",
        "compact_failed",
        "cancelled",
        "timeout",
        "unconfirmed",
        "superseded",
        "internal",
    ] {
        let failure = ControlFailure::from_wire(code).unwrap();
        let outcome: ControlOutcome = Err(failure);
        assert_eq!(failure.as_wire(), code);
        let result = SessionResult {
            command_id: ID.into(),
            result: ControlResult::Terminal(outcome),
        };
        assert_eq!(
            SessionResult::from_tags("❌", &[result.event_tag().unwrap()]),
            Some(result)
        );
    }
    for code in [
        "",
        "future_error",
        "NO_SESSION",
        "/private/path",
        "raw error",
    ] {
        assert!(ControlFailure::from_wire(code).is_none());
    }
}

#[test]
fn snapshot_rejects_invalid_actor_and_automatic_actor() {
    let mut snapshot = envelope();
    snapshot.origin = Some(ControlOrigin {
        command_event_id: ID.into(),
        actor_pubkey: "invalid".into(),
    });
    assert!(snapshot.to_snapshot_bytes().is_err());
    assert!(parse_snapshot(&serde_json::to_vec(&snapshot).unwrap()).is_err());
    snapshot.origin.as_mut().unwrap().actor_pubkey = ACTOR.into();
    snapshot.status.reason = StatusReason::AutoCompact;
    assert!(snapshot.to_snapshot_bytes().is_err());
    assert!(parse_snapshot(&serde_json::to_vec(&snapshot).unwrap()).is_err());
}
