//! Durable admission journeys exercise the same lock as raw ingress and reset.
use super::*;
use crate::{
    authorization_context::{Admission, Journal},
    provider_background::{Identity, Record, SavedEvent},
};

#[test]
fn authorization_admission_reconciles_sources_replay_expiry_and_reset() {
    let dir = tempfile::tempdir().unwrap();
    let owner = nostr::Keys::generate();
    let store = SessionStore::open(
        dir.path().into(),
        "wss://relay.example",
        &owner.public_key().to_hex(),
    )
    .unwrap();
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let source = |text: &str| SavedEvent {
        event: nostr::EventBuilder::new(nostr::Kind::Custom(9), text)
            .tags([nostr::Tag::parse(["h", &scope.channel_id().to_string()]).unwrap()])
            .sign_with_keys(&owner)
            .unwrap(),
        edit: None,
    };
    let original = source("Original scope.");
    // Accept before a provider session exists. Claim/bind must retain this input.
    store
        .accept_authorization_input(&scope, original.clone(), None)
        .unwrap();
    let latest = source("A later accepted input.");
    store
        .accept_authorization_input(&scope, latest.clone(), None)
        .unwrap();
    let expected = Expect::from_outcome(&store.load(&scope));
    let live = LiveSession {
        provider_background: None,
        reply_contract: None,
        lease: Uuid::nil(),
        adapter: "fixture".into(),
        session_id: "session".into(),
        cwd: "/tmp".into(),
        model: None,
        instance: store.instance(),
        worker_index: 0,
        departed: None,
        turns: 0,
        standing_context_sent: false,
        delivered_event_ids: vec![],
        hydrated_thread_roots: vec![],
    };
    let lease = store.claim(&scope, &expected, live).unwrap();
    let mut bg = Record {
        authorization: Some(Journal::new(Some(owner.public_key().to_hex()), true)),
        needs_user: None,
        terminal_reason: None,
        parent_pending: true,
        cancellation: None,
        identity: Identity {
            session_id: "session".into(),
            work_id: Uuid::new_v4(),
            generation: 2,
        },
        scope: scope.clone(),
        thread_tags: Default::default(),
        events: vec![latest.clone()],
        runtime_final: false,
        deadline_unix: crate::provider_background::now_unix() + 60,
        tasks: Default::default(),
        started: Default::default(),
    };
    bg.refresh_authorization().unwrap();
    store.commit_background(&scope, lease, bg).unwrap();
    let load = || {
        let LoadOutcome::Found(record) = store.load(&scope) else {
            panic!("missing work")
        };
        *record.live.unwrap().provider_background.unwrap()
    };
    bg = load();
    assert_eq!(bg.events, vec![original.clone(), latest.clone()]);
    let request = |bg: &Record| Admission {
        identity: bg.identity.clone(),
        revision: bg.authorization.as_ref().unwrap().revision,
        check_id: Uuid::new_v4(),
        call_id: "call".into(),
        fingerprint: "args-and-cwd".into(),
    };
    let first = request(&bg);
    assert_eq!(
        store
            .check_authorization_admission(&scope, lease, &first)
            .unwrap()["status"],
        "stale",
        "end_turn is not an authorization receipt"
    );
    let ack = |mut bg: Record| {
        let journal = bg.authorization.as_mut().unwrap();
        journal.receipt_revision = journal.revision;
        store.commit_background(&scope, lease, bg).unwrap();
    };
    ack(bg);
    assert_eq!(
        store
            .check_authorization_admission(&scope, lease, &first)
            .unwrap()["status"],
        "current"
    );
    let replay = store
        .check_authorization_admission(&scope, lease, &first)
        .unwrap();
    assert_eq!(replay["status"], "current");
    assert_eq!(replay["replayed"], true);
    bg = load();
    let revision = bg.authorization.as_ref().unwrap().revision;
    assert!(!store
        .accept_authorization_input(&scope, original, None)
        .unwrap());
    assert_eq!(load().authorization.as_ref().unwrap().revision, revision);
    let restriction = source("Do not use the previous recipients.");
    store
        .accept_authorization_input(&scope, restriction.clone(), None)
        .unwrap();
    assert_eq!(
        store
            .check_authorization_admission(&scope, lease, &first)
            .unwrap()["status"],
        "stale"
    );
    // An old ACP response must not overwrite a source accepted while it awaited.
    ack(bg);
    bg = load();
    assert!(bg.events.contains(&restriction));
    assert!(
        bg.authorization.as_ref().unwrap().receipt_revision
            < bg.authorization.as_ref().unwrap().revision
    );
    ack(bg);
    bg = load();
    let current = request(&bg);
    assert_eq!(
        store
            .check_authorization_admission(&scope, lease, &current)
            .unwrap()["status"],
        "current"
    );
    let active_snapshot = load();
    // Expiry is checked at admission even before the terminal queue is polled.
    bg.deadline_unix = 0;
    store.commit_background(&scope, lease, bg).unwrap();
    assert_eq!(
        store
            .check_authorization_admission(&scope, lease, &current)
            .unwrap()["status"],
        "stale",
        "expired work still admitted an action"
    );
    bg = load();
    bg.deadline_unix = crate::provider_background::now_unix() + 60;
    store.commit_background(&scope, lease, bg).unwrap();
    let mut conflict = current.clone();
    conflict.fingerprint = "changed arguments".into();
    assert_eq!(
        store
            .check_authorization_admission(&scope, lease, &conflict)
            .unwrap()["status"],
        "stale"
    );
    ack(load());
    assert_eq!(
        store
            .check_authorization_admission(&scope, lease, &request(&load()))
            .unwrap()["status"],
        "stale"
    );
    let foreign = SessionStore::open(
        dir.path().into(),
        "wss://relay.example",
        &owner.public_key().to_hex(),
    )
    .unwrap();
    assert!(matches!(
        foreign.revoke_authorization_for_shutdown(&scope),
        Err(StoreError::Conflict)
    ));
    store.revoke_authorization_for_shutdown(&scope).unwrap();
    assert!(
        load().recoverable(),
        "shutdown discarded pending parent recovery instead of fencing only authorization"
    );
    assert_eq!(
        load().authorization.as_ref().unwrap().snapshot(&load())["state"],
        "revoked"
    );
    let LoadOutcome::Found(before_discard) = store.load(&scope) else {
        panic!("missing work")
    };
    let shutdown_dir = dir.path().join("shutdown-recovery");
    {
        let process = SessionStore::open(
            shutdown_dir.clone(),
            "wss://relay.example",
            &owner.public_key().to_hex(),
        )
        .unwrap();
        let mut shutdown_live = before_discard.live.clone().unwrap();
        shutdown_live.instance = process.instance();
        process
            .claim(&scope, &Expect::Missing, shutdown_live)
            .unwrap();
        process.revoke_authorization_for_shutdown(&scope).unwrap();
    }
    let recovered = SessionStore::open(
        shutdown_dir,
        "wss://relay.example",
        &owner.public_key().to_hex(),
    )
    .unwrap();
    let pending = recovered
        .pending_provider_work(&[scope.channel_id()])
        .unwrap();
    assert_eq!(
        pending.len(),
        1,
        "shutdown registry was omitted from startup recovery"
    );
    assert_eq!(pending[0].identity, current.identity);
    let previous = before_discard
        .live
        .as_ref()
        .unwrap()
        .provider_background
        .as_ref()
        .unwrap();
    assert_eq!(pending[0].deadline_unix, previous.deadline_unix);
    assert_eq!(pending[0].tasks, previous.tasks);
    assert_eq!(
        pending[0]
            .authorization
            .as_ref()
            .unwrap()
            .snapshot(&pending[0])["state"],
        "revoked"
    );
    let mut reusable = before_discard.live.clone().unwrap();
    reusable.provider_background = None;
    store.discard(&scope, lease).unwrap();
    let LoadOutcome::Found(saved) = store.load(&scope) else {
        panic!("missing tombstone")
    };
    assert_eq!(
        saved.authorization_revocation.unwrap().identity,
        current.identity
    );
    assert!(store
        .check_authorization_admission(&scope, lease, &current)
        .is_err());
    let expected = Expect::from_outcome(&store.load(&scope));
    let next_lease = store.claim(&scope, &expected, reusable).unwrap();
    store
        .commit_background(&scope, next_lease, active_snapshot)
        .unwrap();
    assert_eq!(
        store
            .check_authorization_admission(&scope, next_lease, &request(&load()))
            .unwrap()["status"],
        "stale",
        "rebound identity resurrected a revoked grant"
    );
    store
        .fail_writes
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(store
        .accept_authorization_input(&scope, source("A restriction that cannot be saved."), None)
        .is_err());
    assert!(store
        .check_authorization_admission(&scope, next_lease, &current)
        .is_err());
    let reopened = SessionStore::open(
        dir.path().into(),
        "wss://relay.example",
        &owner.public_key().to_hex(),
    )
    .unwrap();
    assert!(reopened
        .check_authorization_admission(&scope, next_lease, &current)
        .is_err());
}

#[test]
fn authorization_source_evidence_and_storage_corruption_never_create_a_grant() {
    let owner = nostr::Keys::generate();
    let agent = nostr::Keys::generate();
    let channel = Uuid::new_v4();
    let owner_hex = owner.public_key().to_hex();
    for (signer, extra, actor, complete) in [
        (&owner, None, "owner_authority", true),
        (&agent, None, "unknown", true),
        (
            &agent,
            Some(nostr::Tag::parse(["auth", owner_hex.as_str()]).unwrap()),
            "agent",
            true,
        ),
        (
            &owner,
            Some(nostr::Tag::parse(["imeta", "url https://example.invalid/opaque"]).unwrap()),
            "owner_authority",
            false,
        ),
    ] {
        let mut tags = vec![nostr::Tag::parse(["h", &channel.to_string()]).unwrap()];
        tags.extend(extra);
        let event = nostr::EventBuilder::new(nostr::Kind::Custom(9), "Raw restrictive input.")
            .tags(tags)
            .sign_with_keys(signer)
            .unwrap();
        let mut bg = Record {
            authorization: Some(Journal::new(Some(owner_hex.clone()), true)),
            needs_user: None,
            terminal_reason: None,
            parent_pending: false,
            cancellation: None,
            identity: Identity {
                session_id: "session".into(),
                work_id: Uuid::new_v4(),
                generation: 1,
            },
            scope: SessionScope::Conversation {
                channel_id: channel,
            },
            thread_tags: Default::default(),
            events: vec![SavedEvent { event, edit: None }],
            runtime_final: false,
            deadline_unix: crate::provider_background::now_unix() + 60,
            tasks: Default::default(),
            started: Default::default(),
        };
        bg.refresh_authorization().unwrap();
        let journal = bg.authorization.as_ref().unwrap();
        let snapshot = journal.snapshot(&bg);
        assert_eq!(snapshot["sources"][0]["actor_kind"], actor);
        assert_eq!(snapshot["coverage"]["complete"], complete);
        assert!(
            snapshot.get("synthetic").is_none(),
            "carrier must not change immutable update payload"
        );
        assert_eq!(
            journal.snapshot(&bg)["revision"],
            snapshot["revision"],
            "collection creates no revision or new grant"
        );
        let mut corrupted = bg.clone();
        corrupted.authorization.as_mut().unwrap().version = 2;
        assert!(
            corrupted.validate().is_err(),
            "unknown persisted schema was accepted"
        );
        corrupted = bg.clone();
        corrupted.authorization.as_mut().unwrap().receipt_revision = u64::MAX;
        assert!(corrupted.validate().is_err(), "future receipt was accepted");
    }
}
