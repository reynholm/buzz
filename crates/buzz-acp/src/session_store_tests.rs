use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

fn thread(root: &str) -> SessionScope {
    SessionScope::Thread {
        channel_id: Uuid::from_u128(7),
        root_event_id: root.repeat(64 / root.len()),
    }
}

fn live(store: &SessionStore, session_id: &str) -> LiveSession {
    LiveSession {
        lease: Uuid::nil(),
        adapter: "claude-agent-acp".into(),
        session_id: session_id.into(),
        cwd: "/work".into(),
        model: Some("opus".into()),
        instance: store.instance(),
        worker_index: 0,
        departed: None,
        turns: 0,
        standing_context_sent: false,
        delivered_event_ids: Vec::new(),
        hydrated_thread_roots: Vec::new(),
    }
}

fn found(outcome: LoadOutcome) -> SessionRecord {
    match outcome {
        LoadOutcome::Found(record) => record,
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn roundtrip_live_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("a");
    let lease = store
        .claim(&scope, &Expect::Missing, live(&store, "s-1"))
        .unwrap();
    let record = found(store.load(&scope));
    let live = record.live.unwrap();
    assert_eq!(live.session_id, "s-1");
    assert_eq!(live.lease, lease);
    assert_eq!(record.reason, None);
}

#[test]
fn corrupt_file_is_renamed_and_reported_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("b");
    let path = store.record_path(&scope);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{").unwrap();
    assert_eq!(store.load(&scope), LoadOutcome::Unreadable);
    assert_eq!(store.load(&scope), LoadOutcome::Missing);
    assert!(with_suffix(&path, "corrupt").exists());
}

#[test]
fn oversized_file_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("c");
    let path = store.record_path(&scope);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, vec![b' '; MAX_RECORD_BYTES as usize + 1]).unwrap();
    assert_eq!(store.load(&scope), LoadOutcome::Unreadable);
}

#[test]
fn claim_conflicts_when_record_changed() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("d");
    let expect = Expect::from_outcome(&store.load(&scope));
    store
        .claim(&scope, &Expect::Missing, live(&store, "s-other"))
        .unwrap();
    assert!(matches!(
        store.claim(&scope, &expect, live(&store, "s-mine")),
        Err(StoreError::Conflict)
    ));
    assert_eq!(found(store.load(&scope)).live.unwrap().session_id, "s-other");
    assert!(!store.is_poisoned(&scope));
}

#[test]
fn late_write_from_previous_lease_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("e");
    let lease_a = store
        .claim(&scope, &Expect::Missing, live(&store, "s-a"))
        .unwrap();
    let expect = Expect::from_outcome(&store.load(&scope));
    // A resume keeps the provider session id but takes a new lease.
    let lease_b = store.claim(&scope, &expect, live(&store, "s-a")).unwrap();
    assert!(matches!(
        store.commit(&scope, lease_a, |live| live.turns = 99),
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store.discard(&scope, lease_a),
        Err(StoreError::Conflict)
    ));
    let live = found(store.load(&scope)).live.unwrap();
    assert_eq!(live.lease, lease_b);
    assert_eq!(live.turns, 0);
}

#[test]
fn concurrent_claims_have_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SessionStore::open(dir.path().to_path_buf()).unwrap());
    let scope = thread("f");
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["s-1", "s-2"]
        .into_iter()
        .map(|sid| {
            let (store, scope, barrier) = (store.clone(), scope.clone(), barrier.clone());
            std::thread::spawn(move || {
                let session = live(&store, sid);
                barrier.wait();
                store.claim(&scope, &Expect::Missing, session).is_ok()
            })
        })
        .collect();
    let wins = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(|won| *won)
        .count();
    assert_eq!(wins, 1);
}

#[test]
fn lock_wait_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("g");
    let holder = store.lock_record(&store.record_path(&scope)).unwrap();
    let started = Instant::now();
    let result = store.claim(&scope, &Expect::Missing, live(&store, "s-1"));
    assert!(matches!(result, Err(StoreError::Busy)));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(store.is_poisoned(&scope));
    drop(holder);
}

#[test]
fn busy_does_not_quarantine_owner_record() {
    let dir = tempfile::tempdir().unwrap();
    let owner = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let other = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("h");
    let lease = owner
        .claim(&scope, &Expect::Missing, live(&owner, "s-a"))
        .unwrap();
    let path = owner.record_path(&scope);
    let holder = owner.lock_record(&path).unwrap();
    other.fail_writes(true);
    assert!(matches!(
        other.discard_scope(&scope),
        Err(StoreError::Busy)
    ));
    drop(holder);
    assert!(path.exists());
    assert!(!with_suffix(&path, "stale").exists());
    assert!(other.is_poisoned(&scope));
    assert_eq!(found(owner.load(&scope)).live.unwrap().lease, lease);
}

#[test]
fn quarantine_requires_matching_lease() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("i");
    store
        .claim(&scope, &Expect::Missing, live(&store, "s-a"))
        .unwrap();
    store.fail_writes(true);
    assert!(matches!(
        store.commit(&scope, Uuid::new_v4(), |live| live.turns = 1),
        Err(StoreError::Conflict)
    ));
    let path = store.record_path(&scope);
    assert!(path.exists());
    assert!(!with_suffix(&path, "stale").exists());
}

#[test]
fn failed_update_of_existing_record_poisons_scope() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("j");
    let lease = store
        .claim(&scope, &Expect::Missing, live(&store, "s-a"))
        .unwrap();
    let channel_dir = store.record_path(&scope).parent().unwrap().to_path_buf();
    fs::set_permissions(&channel_dir, fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.commit(&scope, lease, |live| live.turns = 1);
    fs::set_permissions(&channel_dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(result, Err(StoreError::Io(_))));
    assert!(store.is_poisoned(&scope));
    // The boundary of the guarantee: neither the write nor the rename could
    // happen, so the old record is still there.
    assert_eq!(found(store.load(&scope)).live.unwrap().turns, 0);
}

#[test]
fn failed_discard_moves_record_aside_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path;
    {
        let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
        let scope = thread("k");
        let lease = store
            .claim(&scope, &Expect::Missing, live(&store, "s-a"))
            .unwrap();
        store.fail_writes(true);
        assert!(matches!(
            store.discard(&scope, lease),
            Err(StoreError::Io(_))
        ));
        path = store.record_path(&scope);
    }
    let restarted = SessionStore::open(dir.path().to_path_buf()).unwrap();
    assert_eq!(restarted.load(&thread("k")), LoadOutcome::Missing);
    assert!(with_suffix(&path, "stale").exists());
}

#[test]
fn discard_leaves_tombstone_with_reason() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let scope = thread("l");
    let lease = store
        .claim(&scope, &Expect::Missing, live(&store, "s-a"))
        .unwrap();
    store.discard(&scope, lease).unwrap();
    let record = found(store.load(&scope));
    assert_eq!(record.live, None);
    assert_eq!(record.reason, Some(SessionReason::Discarded));
    assert_eq!(
        Expect::from_outcome(&LoadOutcome::Found(record)),
        Expect::Tombstone
    );
}

#[test]
fn discard_channel_tombstones_only_that_channel() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let ours = [thread("m"), thread("n")];
    let theirs = SessionScope::Conversation {
        channel_id: Uuid::from_u128(8),
    };
    for scope in ours.iter().chain([&theirs]) {
        store
            .claim(scope, &Expect::Missing, live(&store, "s"))
            .unwrap();
    }
    assert_eq!(store.discard_channel(Uuid::from_u128(7)).unwrap(), 2);
    for scope in &ours {
        assert_eq!(found(store.load(scope)).live, None);
    }
    assert!(found(store.load(&theirs)).live.is_some());
}

#[test]
fn other_live_instance_is_alive_dead_one_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let other = SessionStore::open(dir.path().to_path_buf()).unwrap();
    let other_instance = other.instance();
    assert!(store.instance_alive(store.instance()));
    assert!(store.instance_alive(other_instance));
    drop(other);
    assert!(!store.instance_alive(other_instance));
    assert!(!store.instance_alive(Uuid::new_v4()));
}

#[test]
fn append_delivered_caps_oldest_first() {
    let mut ids = Vec::new();
    append_delivered(&mut ids, (0..2050).map(|n| format!("id-{n}")));
    append_delivered(&mut ids, ["id-2049".to_owned()]);
    assert_eq!(ids.len(), MAX_STORED_DELIVERED_IDS);
    assert_eq!(ids[0], "id-2");
    assert_eq!(ids.last().unwrap(), "id-2049");
}

#[test]
fn reason_wire_strings_roundtrip() {
    for reason in [
        SessionReason::IdleSleep,
        SessionReason::HarnessRestart,
        SessionReason::LiveElsewhere,
        SessionReason::ResumeFailed("not_found".into()),
    ] {
        assert_eq!(SessionReason::from(reason.as_wire()), reason);
    }
    assert_eq!(
        SessionReason::ResumeFailed("rpc_-32603".into()).as_wire(),
        "resume_failed:rpc_-32603"
    );
}
