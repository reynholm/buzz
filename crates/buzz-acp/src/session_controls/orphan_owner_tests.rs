//! Autonomous owner reconciliation through the production driver and real locks.
use super::*;
use crate::scope::SessionScope;
use crate::session_store::{ControlReceiptState, LoadOutcome, SessionStore, STORE_LOCK_WAIT};
use fs2::FileExt;

fn fixture() -> (
    tempfile::TempDir,
    SessionStore,
    Arc<SessionStore>,
    SessionScope,
    String,
) {
    let dir = tempfile::tempdir().unwrap();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let owner = SessionStore::open(dir.path().into(), "wss://relay.example", &agent).unwrap();
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let command = "a".repeat(64);
    owner
        .begin_control_attempt(&scope, &command, nostr::Timestamp::now().as_secs())
        .unwrap();
    let next =
        Arc::new(SessionStore::open(dir.path().into(), "wss://relay.example", &agent).unwrap());
    (dir, owner, next, scope, command)
}

async fn assert_eventually_finished(
    reconciliation: &mut OrphanReconciliation,
    controls: &SessionControlRuntime,
    store: &SessionStore,
    scope: &SessionScope,
    command: &str,
) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            reconciliation.wake().await;
            reconciliation.drive(controls, 128);
            let publications = controls.take_publications();
            if !publications.is_empty() {
                assert_eq!(publications.len(), 1);
                assert_eq!(publications[0].command_id, command);
                assert_eq!(
                    publications[0].result,
                    ControlResult::Terminal(Err(ControlFailure::Unconfirmed))
                );
                break;
            }
        }
    })
    .await
    .expect("foreign Started must be revisited without another event or failure");
    assert!(matches!(store.load(scope), LoadOutcome::Found(record)
        if record.control_receipts[0].state == ControlReceiptState::Finished(Err(ControlFailure::Unconfirmed))));
    reconciliation.drive(controls, 128);
    assert!(controls.take_publications().is_empty());
    assert!(reconciliation.next.is_none());
}

#[tokio::test]
async fn orphan_registry_unknown_revisits_after_lock_release_without_external_wake() {
    let (_dir, owner, store, scope, command) = fixture();
    drop(owner);
    let registry = store
        .channel_dir(scope.channel_id())
        .parent()
        .unwrap()
        .join("instances.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(registry)
        .unwrap();
    lock.lock_exclusive().unwrap();
    let controls = SessionControlRuntime::new(Some(store.clone()));
    let mut reconciliation = OrphanReconciliation::default();
    let started = std::time::Instant::now();
    reconciliation.drive(&controls, 128);
    reconciliation.wake().await;
    reconciliation.drive(&controls, 128);
    assert!(started.elapsed() >= STORE_LOCK_WAIT);
    assert!(controls.take_publications().is_empty());
    assert!(matches!(store.load(&scope), LoadOutcome::Found(record)
        if record.control_receipts[0].state == ControlReceiptState::Started));
    drop(lock);
    assert_eventually_finished(&mut reconciliation, &controls, &store, &scope, &command).await;
}

#[tokio::test]
async fn orphan_alive_foreign_owner_is_revisited_after_it_dies() {
    let (_dir, owner, store, scope, command) = fixture();
    let controls = SessionControlRuntime::new(Some(store.clone()));
    let mut reconciliation = OrphanReconciliation::default();
    reconciliation.drive(&controls, 128);
    reconciliation.wake().await;
    reconciliation.drive(&controls, 128);
    assert!(controls.take_publications().is_empty());
    assert!(matches!(store.load(&scope), LoadOutcome::Found(record)
        if record.control_receipts[0].state == ControlReceiptState::Started));
    drop(owner);
    assert_eventually_finished(&mut reconciliation, &controls, &store, &scope, &command).await;
}

#[tokio::test(start_paused = true)]
async fn orphan_live_owner_revisit_uses_capped_backoff_and_never_spins_without_capacity() {
    use futures_util::FutureExt;
    let (_dir, _owner, store, scope, _) = fixture();
    let controls = SessionControlRuntime::new(Some(store.clone()));
    let mut reconciliation = OrphanReconciliation::default();
    reconciliation.drive(&controls, 128);
    for seconds in [1, 2, 4, 8, 16, 32, 60, 60] {
        reconciliation.wake().await;
        reconciliation.drive(&controls, 128);
        assert_eq!(
            reconciliation.next.unwrap() - Instant::now(),
            Duration::from_secs(seconds)
        );
        assert!(controls.take_publications().is_empty());
        assert!(matches!(store.load(&scope), LoadOutcome::Found(record)
            if record.control_receipts[0].state == ControlReceiptState::Started));
        tokio::time::advance(Duration::from_secs(seconds)).await;
        reconciliation.drive(&controls, 0);
        assert!(reconciliation.task.is_none());
        assert!(reconciliation.wake().now_or_never().is_none());
        reconciliation.drive(&controls, 128);
    }
    reconciliation.wake().await;
}
