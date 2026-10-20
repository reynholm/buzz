use super::*;
use buzz_core::session_control::{ControlFailure, ControlOrigin, RestartDepth};

const AGENT: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
fn fixture() -> (tempfile::TempDir, SessionStore, SessionScope) {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    (
        dir,
        store,
        SessionScope::Conversation {
            channel_id: Uuid::from_u128(9),
        },
    )
}
fn id(n: usize) -> String {
    format!("{n:064x}")
}

#[test]
fn admitted_compact_receipt_uses_original_freshness_without_weakening_unadmitted_api() {
    let (_dir, store, scope) = fixture();
    let now = nostr::Timestamp::now().as_secs();
    let signed = now - 901;
    assert!(matches!(
        store.begin_control_attempt(&scope, &id(1), signed),
        Err(StoreError::StaleRequest)
    ));
    assert!(matches!(
        store.begin_admitted_control_with_receipt(&scope, &id(1), signed, now),
        Err(StoreError::StaleRequest)
    ));
    assert!(matches!(
        store.begin_admitted_control_with_receipt(&scope, &id(1), now + 60, now + 60),
        Err(StoreError::StaleRequest)
    ));
    assert_eq!(
        store
            .begin_admitted_control_with_receipt(&scope, &id(1), signed, now - 20)
            .unwrap()
            .0,
        ControlAttempt::New
    );
    assert_eq!(
        record(&store, &scope).control_receipts[0].created_at,
        signed
    );
    assert_eq!(
        store
            .begin_admitted_control_with_receipt(&scope, &id(1), signed, now - 20)
            .unwrap()
            .0,
        ControlAttempt::PriorStarted
    );
}

#[test]
fn compact_terminal_receipt_retry_checks_exact_owner_generation_and_preserves_supersession() {
    let (dir, store, scope) = fixture();
    let now = nostr::Timestamp::now().as_secs();
    store.begin_control_attempt(&scope, &id(1), now).unwrap();
    let expected = record(&store, &scope).control_receipts[0].clone();
    let mut wrong = expected.clone();
    wrong.owner_generation = Uuid::new_v4();
    assert!(matches!(
        store.retry_control_outcome(&scope, &wrong, &Ok(())),
        Err(StoreError::Conflict)
    ));
    assert_eq!(
        record(&store, &scope).control_receipts[0].state,
        ControlReceiptState::Started
    );
    let foreign = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    assert!(matches!(
        foreign.retry_control_outcome(&scope, &expected, &Ok(())),
        Err(StoreError::Conflict)
    ));
    store
        .request_restart_superseding(&scope, &intent(2))
        .unwrap();
    assert_eq!(
        store
            .retry_control_outcome(&scope, &expected, &Ok(()))
            .unwrap(),
        Some(Err(ControlFailure::Superseded))
    );
    assert_eq!(
        record(&store, &scope).control_receipts[0].state,
        ControlReceiptState::Finished(Err(ControlFailure::Superseded))
    );
}
fn intent(n: usize) -> RestartIntent {
    RestartIntent {
        token: Uuid::new_v4(),
        depth: RestartDepth::All,
        origin: ControlOrigin {
            command_event_id: id(n),
            actor_pubkey: id(999),
        },
    }
}
fn record(store: &SessionStore, scope: &SessionScope) -> SessionRecord {
    match store.load(scope) {
        LoadOutcome::Found(r) => *r,
        other => panic!("{other:?}"),
    }
}
fn live(store: &SessionStore) -> LiveSession {
    LiveSession {
        lease: Uuid::nil(),
        adapter: "fake".into(),
        session_id: "existing".into(),
        cwd: "/tmp".into(),
        model: None,
        instance: store.instance(),
        worker_index: 0,
        departed: None,
        turns: 0,
        standing_context_sent: false,
        delivered_event_ids: vec![],
        hydrated_thread_roots: vec![],
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn restart_intent_is_atomic() {
    let (_dir, store, scope) = fixture();
    let lease = store.claim(&scope, &Expect::Missing, live(&store)).unwrap();
    let restart = intent(1);
    store.request_restart(&scope, &restart).unwrap();
    let stored = record(&store, &scope);
    assert!(stored.live.is_none());
    assert_eq!(stored.pending_restart, Some(restart));
    assert!(stored.revision.is_some());
    assert!(matches!(
        store.commit(&scope, lease, |_| {}),
        Err(StoreError::Conflict)
    ));
}

#[test]
fn legacy_v2_record_keeps_live_session() {
    let (_dir, store, scope) = fixture();
    store.claim(&scope, &Expect::Missing, live(&store)).unwrap();
    let mut json = serde_json::to_value(record(&store, &scope)).unwrap();
    for field in ["revision", "pending_restart", "control_receipts"] {
        json.as_object_mut().unwrap().remove(field);
    }
    fs::write(
        store.record_path(&scope),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap();
    let legacy = record(&store, &scope);
    assert!(legacy.live.is_some());
    assert_eq!(legacy.revision, None);
    assert!(legacy.control_receipts.is_empty());
    store
        .claim(
            &scope,
            &Expect::from_outcome(&store.load(&scope)),
            live(&store),
        )
        .unwrap();
    assert!(record(&store, &scope).revision.is_some());
}

#[test]
fn tombstone_revision_rejects_aba() {
    let (_dir, store, scope) = fixture();
    store.discard_scope(&scope).unwrap();
    let old = Expect::from_outcome(&store.load(&scope));
    store.request_restart(&scope, &intent(1)).unwrap();
    assert!(matches!(
        store.claim(&scope, &old, live(&store)),
        Err(StoreError::Conflict)
    ));
}

#[test]
fn claim_preserves_pending_restart_and_commit_does_not_clear_it() {
    let (_dir, store, scope) = fixture();
    let restart = intent(1);
    store.request_restart(&scope, &restart).unwrap();
    let lease = store
        .claim(
            &scope,
            &Expect::from_outcome(&store.load(&scope)),
            live(&store),
        )
        .unwrap();
    store.commit(&scope, lease, |live| live.turns += 1).unwrap();
    assert_eq!(
        record(&store, &scope).pending_restart,
        Some(restart.clone())
    );
    store.complete_restart(&scope, restart.token).unwrap();
    assert_eq!(record(&store, &scope).pending_restart, None);
}

#[test]
fn failed_attempt_discard_preserves_restart_until_matching_completion() {
    for depth in [RestartDepth::None, RestartDepth::Recent, RestartDepth::All] {
        let (_dir, store, scope) = fixture();
        let mut restart = intent(1);
        restart.depth = depth;
        store.request_restart(&scope, &restart).unwrap();
        let receipts = record(&store, &scope).control_receipts;
        let first_lease = store
            .claim(
                &scope,
                &Expect::from_outcome(&store.load(&scope)),
                live(&store),
            )
            .unwrap();
        // Existing initial-message error/cancel cleanup discards this lease
        // before the foreground assignment has been delivered successfully.
        store.discard(&scope, first_lease).unwrap();
        let discarded = record(&store, &scope);
        assert!(discarded.live.is_none());
        assert_eq!(discarded.pending_restart, Some(restart.clone()));
        assert_eq!(discarded.control_receipts, receipts);
        assert!(matches!(
            store.commit(&scope, first_lease, |_| {}),
            Err(StoreError::Conflict)
        ));
        store
            .claim(
                &scope,
                &Expect::from_outcome(&store.load(&scope)),
                live(&store),
            )
            .unwrap();
        assert_eq!(
            record(&store, &scope).pending_restart,
            Some(restart.clone())
        );
        assert!(matches!(
            store.complete_restart(&scope, Uuid::new_v4()),
            Err(StoreError::Conflict)
        ));
        assert_eq!(
            record(&store, &scope).pending_restart,
            Some(restart.clone())
        );
        store.complete_restart(&scope, restart.token).unwrap();
        assert_eq!(record(&store, &scope).pending_restart, None);
        assert_eq!(record(&store, &scope).control_receipts, receipts);
    }
}

#[test]
fn explicit_scope_and_channel_revocation_clear_intent_but_keep_receipts() {
    for channel_wide in [false, true] {
        let (_dir, store, scope) = fixture();
        let restart = intent(1);
        store.request_restart(&scope, &restart).unwrap();
        let receipts = record(&store, &scope).control_receipts;
        let lease = store
            .claim(
                &scope,
                &Expect::from_outcome(&store.load(&scope)),
                live(&store),
            )
            .unwrap();
        if channel_wide {
            store.discard_channel(scope.channel_id()).unwrap();
        } else {
            store.discard_scope(&scope).unwrap();
        }
        // A late failed attempt cannot restore a deliberately revoked policy.
        assert!(matches!(
            store.discard(&scope, lease),
            Err(StoreError::Conflict)
        ));
        let revoked = record(&store, &scope);
        assert!(revoked.live.is_none());
        assert_eq!(revoked.pending_restart, None);
        assert_eq!(revoked.control_receipts, receipts);
    }
}

#[test]
fn old_token_cannot_clear_newer_intent() {
    let (_dir, store, scope) = fixture();
    let old = intent(1);
    let new = intent(2);
    store.request_restart(&scope, &old).unwrap();
    store.request_restart(&scope, &new).unwrap();
    assert!(matches!(
        store.complete_restart(&scope, old.token),
        Err(StoreError::Conflict)
    ));
    assert_eq!(record(&store, &scope).pending_restart, Some(new));
    // Redelivery of the older command must not fence the new intent again.
    store.request_restart(&scope, &old).unwrap();
    assert_eq!(
        record(&store, &scope)
            .pending_restart
            .as_ref()
            .unwrap()
            .origin
            .command_event_id,
        id(2)
    );
}

#[test]
fn started_compact_redelivery_never_reexecutes() {
    let (_dir, store, scope) = fixture();
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        ControlAttempt::New
    );
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        ControlAttempt::PriorStarted
    );
    store
        .finish_control_attempt(&scope, &id(1), &Err(ControlFailure::Unconfirmed))
        .unwrap();
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        ControlAttempt::PriorFinished(Err(ControlFailure::Unconfirmed))
    );
    assert!(matches!(
        store.finish_control_attempt(&scope, &id(1), &Ok(())),
        Err(StoreError::Conflict)
    ));
}

#[test]
fn restart_started_receipt_is_written_with_tombstone_intent() {
    let (_dir, store, scope) = fixture();
    store.claim(&scope, &Expect::Missing, live(&store)).unwrap();
    let before = fs::read(store.record_path(&scope)).unwrap();
    let restart = intent(1);
    store.fail_writes(true);
    assert!(store.request_restart(&scope, &restart).is_err());
    assert_eq!(fs::read(store.record_path(&scope)).unwrap(), before);
    store.fail_writes(false);
    store.request_restart(&scope, &restart).unwrap();
    let stored = record(&store, &scope);
    assert!(stored.live.is_none());
    assert_eq!(stored.pending_restart, Some(restart));
    assert_eq!(stored.control_receipts.len(), 1);
    assert_eq!(
        stored.control_receipts[0].state,
        ControlReceiptState::Started
    );
    assert_eq!(stored.control_receipts[0].owner_instance, store.instance());
}

#[test]
fn compact_receipt_survives_restart_rotate_channel_tombstone() {
    let (_dir, store, scope) = fixture();
    store.begin_control_attempt(&scope, &id(1), now()).unwrap();
    store
        .finish_control_attempt(&scope, &id(1), &Ok(()))
        .unwrap();
    let expected = ControlAttempt::PriorFinished(Ok(()));
    store.request_restart(&scope, &intent(2)).unwrap();
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        expected
    );
    let lease = store
        .claim(
            &scope,
            &Expect::from_outcome(&store.load(&scope)),
            live(&store),
        )
        .unwrap();
    store.discard(&scope, lease).unwrap();
    store.discard_scope(&scope).unwrap();
    store.discard_channel(scope.channel_id()).unwrap();
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        expected
    );
}

#[test]
fn orphan_started_becomes_terminal_without_reexecution() {
    let (dir, store, scope) = fixture();
    store.begin_control_attempt(&scope, &id(1), now()).unwrap();
    let receipt = record(&store, &scope).control_receipts[0].clone();
    let observer = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    assert!(matches!(
        observer.reconcile_control_attempt(
            &scope,
            &id(1),
            receipt.owner_instance,
            receipt.owner_generation
        ),
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store.reconcile_control_attempt(&scope, &id(1), receipt.owner_instance, Uuid::new_v4()),
        Err(StoreError::Conflict)
    ));
    drop(store);
    observer
        .reconcile_control_attempt(
            &scope,
            &id(1),
            receipt.owner_instance,
            receipt.owner_generation,
        )
        .unwrap();
    assert_eq!(
        observer
            .begin_control_attempt(&scope, &id(1), now())
            .unwrap(),
        ControlAttempt::PriorFinished(Err(ControlFailure::Unconfirmed))
    );
}

#[test]
fn receipts_are_bounded_fresh_and_pending_never_evicted() {
    let (_dir, store, scope) = fixture();
    for n in 0..128 {
        store.begin_control_attempt(&scope, &id(n), now()).unwrap();
        store
            .finish_control_attempt(&scope, &id(n), &Ok(()))
            .unwrap();
    }
    assert!(matches!(
        store.begin_control_attempt(&scope, &id(128), now()),
        Err(StoreError::Busy)
    ));
    assert!(matches!(
        store.begin_control_attempt(&scope, &id(129), now() - 901),
        Err(StoreError::StaleRequest)
    ));
    assert!(matches!(
        store.begin_control_attempt(&scope, &id(129), now() + 31),
        Err(StoreError::StaleRequest)
    ));
    let path = store.record_path(&scope);
    let mut stored = record(&store, &scope);
    stored.control_receipts[0].created_at = now() - 931;
    stored.control_receipts[1].created_at = now() - 931;
    stored.control_receipts[1].state = ControlReceiptState::Started;
    fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
    assert_eq!(
        store
            .begin_control_attempt(&scope, &id(128), now())
            .unwrap(),
        ControlAttempt::New
    );
    let stored = record(&store, &scope);
    assert_eq!(stored.control_receipts.len(), 128);
    assert!(stored
        .control_receipts
        .iter()
        .any(|r| r.command_id == id(1)));
    assert!(!stored
        .control_receipts
        .iter()
        .any(|r| r.command_id == id(0)));
}

#[test]
fn receipt_write_and_lock_failures_do_not_allow_execution() {
    let (_dir, store, scope) = fixture();
    store.fail_writes(true);
    assert!(store.begin_control_attempt(&scope, &id(1), now()).is_err());
    store.fail_writes(false);
    let lock = store.lock_record(&store.record_path(&scope)).unwrap();
    assert!(matches!(
        store.begin_control_attempt(&scope, &id(1), now()),
        Err(StoreError::Busy)
    ));
    drop(lock);
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        ControlAttempt::New
    );
}

#[test]
fn foreign_community_and_agent_cannot_adopt_control_metadata() {
    let (dir, store, scope) = fixture();
    store.request_restart(&scope, &intent(1)).unwrap();
    for (relay, agent) in [
        ("wss://other.example", AGENT.to_owned()),
        (
            "wss://relay.example",
            nostr::Keys::generate().public_key().to_hex(),
        ),
    ] {
        let foreign = SessionStore::open(dir.path().into(), relay, &agent).unwrap();
        let path = foreign.record_path(&scope);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(store.record_path(&scope), &path).unwrap();
        assert!(matches!(
            foreign.begin_control_attempt(&scope, &id(1), now()),
            Err(StoreError::Unreadable)
        ));
    }
}

#[test]
fn quarantine_retains_control_metadata_without_recovering_provider_state() {
    let (dir, store, scope) = fixture();
    let restart = intent(1);
    store.request_restart(&scope, &restart).unwrap();
    store
        .finish_control_attempt(&scope, &id(1), &Ok(()))
        .unwrap();
    let lease = store
        .claim(
            &scope,
            &Expect::from_outcome(&store.load(&scope)),
            live(&store),
        )
        .unwrap();
    store.fail_writes(true);
    assert!(store.commit(&scope, lease, |live| live.turns += 1).is_err());
    drop(store);
    let store = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    let recovered = record(&store, &scope);
    assert_eq!(recovered.live, None);
    assert_eq!(recovered.pending_restart, Some(restart.clone()));
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        ControlAttempt::PriorFinished(Ok(()))
    );
    // A canonical successful clear always wins over older quarantine metadata.
    store.complete_restart(&scope, restart.token).unwrap();
    assert_eq!(record(&store, &scope).pending_restart, None);
    fs::write(store.record_path(&scope), b"corrupt newer record").unwrap();
    assert_eq!(store.load(&scope), LoadOutcome::Unreadable);
    assert_eq!(store.load(&scope), LoadOutcome::Missing);
}

#[test]
fn channel_discard_includes_quarantined_controls_and_keeps_receipts() {
    let (_dir, store, scope) = fixture();
    let restart = intent(1);
    store.request_restart(&scope, &restart).unwrap();
    let lease = store
        .claim(
            &scope,
            &Expect::from_outcome(&store.load(&scope)),
            live(&store),
        )
        .unwrap();
    store.fail_writes(true);
    assert!(store.commit(&scope, lease, |_| {}).is_err());
    store.fail_writes(false);
    assert_eq!(store.discard_channel(scope.channel_id()).unwrap(), 1);
    let stored = record(&store, &scope);
    assert!(stored.pending_restart.is_none());
    assert_eq!(stored.control_receipts.len(), 1);
    assert!(stored.live.is_none());
}

#[test]
fn same_instance_reconciliation_requires_exact_command_and_generation() {
    let (_dir, store, scope) = fixture();
    store.begin_control_attempt(&scope, &id(1), now()).unwrap();
    let receipt = record(&store, &scope).control_receipts[0].clone();
    assert!(matches!(
        store.reconcile_control_attempt(
            &scope,
            &id(2),
            receipt.owner_instance,
            receipt.owner_generation
        ),
        Err(StoreError::Conflict)
    ));
    store
        .reconcile_control_attempt(
            &scope,
            &id(1),
            receipt.owner_instance,
            receipt.owner_generation,
        )
        .unwrap();
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        ControlAttempt::PriorFinished(Err(ControlFailure::Unconfirmed))
    );
}

#[test]
fn failed_finish_preserves_started_and_failed_begin_preserves_previous_receipts() {
    let (_dir, store, scope) = fixture();
    store.begin_control_attempt(&scope, &id(1), now()).unwrap();
    store.fail_writes(true);
    assert!(store
        .finish_control_attempt(&scope, &id(1), &Ok(()))
        .is_err());
    assert!(store.begin_control_attempt(&scope, &id(2), now()).is_err());
    store.fail_writes(false);
    assert_eq!(
        store.begin_control_attempt(&scope, &id(1), now()).unwrap(),
        ControlAttempt::PriorStarted
    );
    assert_eq!(record(&store, &scope).control_receipts.len(), 1);
}

#[test]
fn metadata_survives_store_reopen_and_foreign_quarantine_is_rejected() {
    let (dir, store, scope) = fixture();
    let restart = intent(1);
    store.request_restart(&scope, &restart).unwrap();
    drop(store);
    let store = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    assert_eq!(record(&store, &scope).pending_restart, Some(restart));
    let foreign = SessionStore::open(dir.path().into(), "wss://foreign.example", AGENT).unwrap();
    let path = foreign.record_path(&scope);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::copy(store.record_path(&scope), with_suffix(&path, "stale")).unwrap();
    assert_eq!(foreign.load(&scope), LoadOutcome::Unreadable);
    assert_eq!(foreign.load(&scope), LoadOutcome::Missing);
}

#[test]
fn concurrent_duplicate_starts_have_exactly_one_new_admission() {
    let (_dir, store, scope) = fixture();
    let admissions = std::thread::scope(|threads| {
        let jobs: Vec<_> = (0..8)
            .map(|_| threads.spawn(|| store.begin_control_attempt(&scope, &id(1), now()).unwrap()))
            .collect();
        jobs.into_iter()
            .map(|j| j.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        admissions
            .iter()
            .filter(|a| **a == ControlAttempt::New)
            .count(),
        1
    );
    assert_eq!(
        admissions
            .iter()
            .filter(|a| **a == ControlAttempt::PriorStarted)
            .count(),
        7
    );
}

#[test]
fn malformed_or_oversized_receipt_records_are_rejected() {
    let (_dir, store, scope) = fixture();
    store.begin_control_attempt(&scope, &id(1), now()).unwrap();
    let original = record(&store, &scope);
    for invalid in 0..3 {
        let mut stored = original.clone();
        match invalid {
            0 => stored.control_receipts[0].command_id = "invalid".into(),
            1 => stored
                .control_receipts
                .push(stored.control_receipts[0].clone()),
            _ => {
                stored.control_receipts = (0..129)
                    .map(|n| {
                        let mut r = original.control_receipts[0].clone();
                        r.command_id = id(n);
                        r
                    })
                    .collect();
            }
        }
        // The writer must reject invalid metadata before replacing a record;
        // reader validation alone cannot hide a successful invalid write.
        let path = store.record_path(&scope);
        let before = fs::read(&path).ok();
        assert!(store.write_record(&path, &stored).is_err());
        assert_eq!(fs::read(&path).ok(), before);
        fs::write(
            store.record_path(&scope),
            serde_json::to_vec(&stored).unwrap(),
        )
        .unwrap();
        assert_eq!(store.load(&scope), LoadOutcome::Unreadable);
    }
    assert!(matches!(
        store.begin_control_attempt(&scope, "invalid", now()),
        Err(StoreError::InvalidControl)
    ));
    let mut oversized = live(&store);
    oversized.cwd = "x".repeat(MAX_RECORD_BYTES as usize);
    assert!(matches!(
        store.claim(&scope, &Expect::Missing, oversized),
        Err(StoreError::Io(err)) if err.kind() == io::ErrorKind::InvalidData
    ));
    assert!(!store.record_path(&scope).exists());
    let mut bad_restart = intent(3);
    bad_restart.origin.actor_pubkey = "invalid".into();
    assert!(matches!(
        store.request_restart(&scope, &bad_restart),
        Err(StoreError::InvalidControl)
    ));
}

#[test]
fn startup_sweep_terminalizes_dead_started_controls_but_preserves_live_owner() {
    let (dir, first, scope) = fixture();
    first.request_restart(&scope, &intent(1)).unwrap();
    let second = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    assert!(second
        .sweep_orphan_controls(128)
        .unwrap()
        .finished
        .is_empty());
    drop(first);
    assert_eq!(
        second.sweep_orphan_controls(128).unwrap().finished,
        vec![id(1)]
    );
    assert_eq!(
        record(&second, &scope).control_receipts[0].state,
        ControlReceiptState::Finished(Err(ControlFailure::Unconfirmed))
    );
    assert!(
        record(&second, &scope).pending_restart.is_some(),
        "restart depth survives orphan reconciliation"
    );
    assert!(second
        .sweep_orphan_controls(128)
        .unwrap()
        .finished
        .is_empty());
}

#[test]
fn startup_sweep_chunks_orphan_outcomes_without_losing_remaining_started_receipts() {
    let (dir, first, scope) = fixture();
    let scopes = [
        scope,
        SessionScope::Conversation {
            channel_id: Uuid::from_u128(10),
        },
        SessionScope::Conversation {
            channel_id: Uuid::from_u128(11),
        },
    ];
    for (index, scope) in scopes.iter().enumerate() {
        first.request_restart(scope, &intent(index + 1)).unwrap();
    }
    drop(first);
    let second = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    let a = second.sweep_orphan_controls(2).unwrap().finished;
    let b = second.sweep_orphan_controls(2).unwrap().finished;
    assert_eq!((a.len(), b.len()), (2, 1));
    assert_eq!(a.into_iter().chain(b).collect::<HashSet<_>>().len(), 3);
    assert!(second.sweep_orphan_controls(2).unwrap().finished.is_empty());
    for scope in scopes {
        assert!(record(&second, &scope)
            .control_receipts
            .iter()
            .all(|receipt| receipt.state
                == ControlReceiptState::Finished(Err(ControlFailure::Unconfirmed))));
    }
}

#[test]
fn startup_sweep_keeps_healthy_and_committed_prefix_outcomes_across_bad_records() {
    for busy in [false, true] {
        let (dir, first, scope) = fixture();
        let mut scopes: Vec<_> = (0..3)
            .map(|index| SessionScope::Thread {
                channel_id: scope.channel_id(),
                root_event_id: format!("{index:064x}"),
            })
            .collect();
        scopes.sort_by_key(|scope| first.record_path(scope));
        for (index, scope) in scopes.iter().enumerate() {
            first.request_restart(scope, &intent(index + 1)).unwrap();
        }
        let observed_order: Vec<_> = fs::read_dir(first.channel_dir(scope.channel_id()))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .collect();
        let bad_path = observed_order[1].clone();
        let prefix_scope = scopes
            .iter()
            .find(|scope| first.record_path(scope) == observed_order[0])
            .unwrap()
            .clone();
        let bad_scope = scopes
            .iter()
            .find(|scope| first.record_path(scope) == bad_path)
            .unwrap()
            .clone();
        let bad_id = record(&first, &bad_scope).control_receipts[0]
            .command_id
            .clone();
        let expected: HashSet<_> = scopes
            .iter()
            .filter(|scope| **scope != bad_scope)
            .map(|scope| record(&first, scope).control_receipts[0].command_id.clone())
            .collect();
        let good_bytes = fs::read(&bad_path).unwrap();
        drop(first);
        let second = std::sync::Arc::new(
            SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap(),
        );
        let lock = if busy {
            Some(second.lock_record(&bad_path).unwrap())
        } else {
            fs::write(&bad_path, b"corrupt").unwrap();
            None
        };
        let controls = crate::session_controls::SessionControlRuntime::new(Some(second.clone()));
        let result = controls.sweep_orphans(128);
        assert!(result.is_ok(), "one bad record must not discard the committed prefix or abandon healthy scopes: {result:?}");
        let results: HashSet<_> = controls
            .take_publications()
            .into_iter()
            .map(|item| item.command_id)
            .collect();
        assert_eq!(results, expected);
        assert!(record(&second, &prefix_scope)
            .control_receipts
            .iter()
            .all(|r| matches!(r.state, ControlReceiptState::Finished(_))));
        if busy {
            drop(lock);
        } else {
            fs::write(&bad_path, good_bytes).unwrap();
        }
        controls.sweep_orphans(128).unwrap();
        assert_eq!(
            controls
                .take_publications()
                .into_iter()
                .map(|p| p.command_id)
                .collect::<Vec<_>>(),
            vec![bad_id]
        );
    }
}

#[test]
fn startup_sweep_recovers_stale_only_started_receipt_and_deduplicates_record_key() {
    let (dir, first, scope) = fixture();
    first.request_restart(&scope, &intent(1)).unwrap();
    let path = first.record_path(&scope);
    fs::rename(&path, with_suffix(&path, "stale")).unwrap();
    drop(first);
    let second = SessionStore::open(dir.path().into(), "wss://relay.example", AGENT).unwrap();
    assert_eq!(
        second.sweep_orphan_controls(128).unwrap().finished,
        vec![id(1)]
    );
    assert!(record(&second, &scope).pending_restart.is_some());
    assert!(
        path.exists(),
        "existing reader and locked writer recover the canonical key"
    );
    assert!(
        second
            .sweep_orphan_controls(128)
            .unwrap()
            .finished
            .is_empty(),
        "canonical plus stale must not publish twice"
    );
}
