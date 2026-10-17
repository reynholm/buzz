use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

const TEST_AGENT: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
fn open_store(root: PathBuf) -> io::Result<SessionStore> {
    SessionStore::open(root, "wss://relay.example", TEST_AGENT)
}

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
        LoadOutcome::Found(record) => *record,
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn roundtrip_live_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path().to_path_buf()).unwrap();
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
    let store = open_store(dir.path().to_path_buf()).unwrap();
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
    let store = open_store(dir.path().to_path_buf()).unwrap();
    let scope = thread("c");
    let path = store.record_path(&scope);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, vec![b' '; MAX_RECORD_BYTES as usize + 1]).unwrap();
    assert_eq!(store.load(&scope), LoadOutcome::Unreadable);
}

#[test]
fn claim_conflicts_when_record_changed() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path().to_path_buf()).unwrap();
    let scope = thread("d");
    let expect = Expect::from_outcome(&store.load(&scope));
    store
        .claim(&scope, &Expect::Missing, live(&store, "s-other"))
        .unwrap();
    assert!(matches!(
        store.claim(&scope, &expect, live(&store, "s-mine")),
        Err(StoreError::Conflict)
    ));
    assert_eq!(
        found(store.load(&scope)).live.unwrap().session_id,
        "s-other"
    );
    assert!(!store.is_poisoned(&scope));
}

#[test]
fn late_write_from_previous_lease_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path().to_path_buf()).unwrap();
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
    let store = Arc::new(open_store(dir.path().to_path_buf()).unwrap());
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
    let store = open_store(dir.path().to_path_buf()).unwrap();
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
    let owner = open_store(dir.path().to_path_buf()).unwrap();
    let other = open_store(dir.path().to_path_buf()).unwrap();
    let scope = thread("h");
    let lease = owner
        .claim(&scope, &Expect::Missing, live(&owner, "s-a"))
        .unwrap();
    let path = owner.record_path(&scope);
    let holder = owner.lock_record(&path).unwrap();
    other.fail_writes(true);
    assert!(matches!(other.discard_scope(&scope), Err(StoreError::Busy)));
    drop(holder);
    assert!(path.exists());
    assert!(!with_suffix(&path, "stale").exists());
    assert!(other.is_poisoned(&scope));
    assert_eq!(found(owner.load(&scope)).live.unwrap().lease, lease);
}

#[test]
fn quarantine_requires_matching_lease() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path().to_path_buf()).unwrap();
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
    let store = open_store(dir.path().to_path_buf()).unwrap();
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
        let store = open_store(dir.path().to_path_buf()).unwrap();
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
    let restarted = open_store(dir.path().to_path_buf()).unwrap();
    assert_eq!(restarted.load(&thread("k")), LoadOutcome::Missing);
    assert!(with_suffix(&path, "stale").exists());
}

#[test]
fn discard_leaves_tombstone_with_reason() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path().to_path_buf()).unwrap();
    let scope = thread("l");
    let lease = store
        .claim(&scope, &Expect::Missing, live(&store, "s-a"))
        .unwrap();
    store.discard(&scope, lease).unwrap();
    let record = found(store.load(&scope));
    assert_eq!(record.live, None);
    assert_eq!(record.reason, Some(SessionReason::Discarded));
    assert_eq!(
        Expect::from_outcome(&LoadOutcome::Found(Box::new(record))),
        Expect::Tombstone
    );
}

#[test]
fn discard_channel_tombstones_only_that_channel() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path().to_path_buf()).unwrap();
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
    let store = open_store(dir.path().to_path_buf()).unwrap();
    let other = open_store(dir.path().to_path_buf()).unwrap();
    // An instance registers on its first write; only then can it own records.
    other
        .claim(&thread("r"), &Expect::Missing, live(&other, "s"))
        .unwrap();
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

#[test]
fn open_does_not_touch_disk_until_first_write() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("store");
    let store = open_store(root.clone()).unwrap();
    assert!(!root.exists());
    assert_eq!(store.load(&thread("o")), LoadOutcome::Missing);
    assert!(!root.exists());
    store
        .claim(&thread("o"), &Expect::Missing, live(&store, "s"))
        .unwrap();
    assert!(store
        .dir
        .join(INSTANCES_DIR)
        .join(format!("{}.lock", store.instance()))
        .exists());
}

#[test]
fn dead_instance_locks_are_swept_and_own_lock_removed_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let first = open_store(dir.path().to_path_buf()).unwrap();
    first
        .claim(&thread("p"), &Expect::Missing, live(&first, "s"))
        .unwrap();
    let first_lock = first
        .dir
        .join(INSTANCES_DIR)
        .join(format!("{}.lock", first.instance()));
    // A crashed process leaves its lock file behind but no longer holds it.
    let crashed = first
        .dir
        .join(INSTANCES_DIR)
        .join(format!("{}.lock", Uuid::new_v4()));
    fs::write(&crashed, b"").unwrap();
    let second = open_store(dir.path().to_path_buf()).unwrap();
    second
        .claim(&thread("q"), &Expect::Missing, live(&second, "s"))
        .unwrap();
    assert!(!crashed.exists(), "dead instance lock is swept");
    assert!(first_lock.exists(), "live instance lock is kept");
    drop(first);
    assert!(!first_lock.exists(), "own lock is removed on drop");
}

#[test]
fn instance_publication_cannot_be_reaped_before_locking() {
    let dir = tempfile::tempdir().unwrap();
    let first = Arc::new(open_store(dir.path().to_path_buf()).unwrap());
    let second = open_store(dir.path().to_path_buf()).unwrap();
    let (opened_tx, opened_rx) = std::sync::mpsc::sync_channel(0);
    let (continue_tx, continue_rx) = std::sync::mpsc::sync_channel(0);
    *first.before_instance_lock.lock().unwrap() = Some(Box::new(move || {
        opened_tx.send(()).unwrap();
        continue_rx.recv().unwrap();
    }));
    let writer = first.clone();
    let claim = std::thread::spawn(move || {
        writer.claim(&thread("a"), &Expect::Missing, live(&writer, "active"))
    });
    opened_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let alive_while_publishing = second.instance_alive(first.instance());
    continue_tx.send(()).unwrap();
    claim.join().unwrap().unwrap();
    assert!(
        alive_while_publishing,
        "publication must fail closed while registration owns the registry"
    );
    assert!(
        second.instance_alive(first.instance()),
        "registered owner must remain discoverable"
    );
}

#[test]
fn channel_enumeration_failure_poisons_unknown_scopes_until_restart() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path().to_path_buf()).unwrap();
    let scope = thread("t");
    let neighbor = SessionScope::Conversation {
        channel_id: Uuid::from_u128(8),
    };
    for scope in [&scope, &neighbor] {
        store
            .claim(scope, &Expect::Missing, live(&store, "before-removal"))
            .unwrap();
    }
    let channel_dir = store.record_path(&scope).parent().unwrap().to_path_buf();
    fs::set_permissions(&channel_dir, fs::Permissions::from_mode(0o000)).unwrap();
    let result = store.discard_channel(scope.channel_id());
    fs::set_permissions(&channel_dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(result, Err(StoreError::Io(_))));
    assert!(
        store.is_poisoned(&scope),
        "failed removal must forbid resume in this process"
    );
    assert!(
        store.is_poisoned(&thread("u")),
        "unknown scopes in the same channel are fenced too"
    );
    assert!(!store.is_poisoned(&neighbor));
}

#[test]
fn namespace_normalizes_url_spelling_without_aliasing_communities() {
    let root = tempfile::tempdir().unwrap();
    let a = SessionStore::open(root.path().into(), "WSS://Relay.Example:443/", TEST_AGENT).unwrap();
    let same = SessionStore::open(root.path().into(), "wss://relay.example", TEST_AGENT).unwrap();
    assert_eq!(a.dir, same.dir);
    for relay in [
        "ws://relay.example",
        "wss://relay.example:444",
        "wss://other.example",
        "wss://relay.example/community",
        "wss://relay.example/?community=2",
    ] {
        let other = SessionStore::open(root.path().into(), relay, TEST_AGENT).unwrap();
        assert_ne!(a.dir, other.dir, "distinct relay: {relay}");
    }
    let local = SessionStore::open(root.path().into(), "ws://localhost:3030", TEST_AGENT).unwrap();
    let ip = SessionStore::open(root.path().into(), "ws://127.0.0.1:3030", TEST_AGENT).unwrap();
    assert_ne!(
        local.dir, ip.dir,
        "host-derived communities must remain distinct"
    );
    let agent = nostr::Keys::generate().public_key().to_hex();
    let other = SessionStore::open(root.path().into(), "wss://relay.example", &agent).unwrap();
    assert_ne!(a.dir, other.dir, "configured roots isolate agents too");
}

#[test]
fn copied_foreign_namespace_record_is_not_resumable() {
    let root = tempfile::tempdir().unwrap();
    let a = SessionStore::open(root.path().into(), "wss://a.example", TEST_AGENT).unwrap();
    let b = SessionStore::open(root.path().into(), "wss://b.example", TEST_AGENT).unwrap();
    let scope = thread("v");
    a.claim(&scope, &Expect::Missing, live(&a, "community-a"))
        .unwrap();
    let foreign_path = b.record_path(&scope);
    fs::create_dir_all(foreign_path.parent().unwrap()).unwrap();
    fs::copy(a.record_path(&scope), &foreign_path).unwrap();
    assert_eq!(b.load(&scope), LoadOutcome::Unreadable);
    assert!(with_suffix(&foreign_path, "corrupt").exists());
    assert_eq!(
        found(a.load(&scope)).live.unwrap().session_id,
        "community-a"
    );
}

#[test]
fn legacy_unscoped_records_are_not_adopted() {
    let root = tempfile::tempdir().unwrap();
    let store = open_store(root.path().into()).unwrap();
    let scope = thread("w");
    let name = store.record_path(&scope).file_name().unwrap().to_owned();
    let legacy = serde_json::json!({"version":1, "live":live(&store,"legacy"), "reason":null});
    for dir in [root.path().to_path_buf(), root.path().join(TEST_AGENT)] {
        let path = dir.join(scope.channel_id().to_string()).join(&name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(store.load(&scope), LoadOutcome::Missing);
        assert!(path.exists(), "legacy records are left intact");
    }
    let path = store.record_path(&scope);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(store.load(&scope), LoadOutcome::Unreadable);
}

#[test]
fn invalid_identity_disables_store_instead_of_sharing_a_namespace() {
    let root = tempfile::tempdir().unwrap();
    for relay in [
        "not-a-url",
        "https://relay.example",
        "wss://user:pass@relay.example",
        "wss://relay.example/#fragment",
    ] {
        assert!(SessionStore::open(root.path().into(), relay, TEST_AGENT).is_err());
    }
    assert!(SessionStore::open(root.path().into(), "wss://relay.example", "invalid-key").is_err());
}
