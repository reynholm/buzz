//! Deployment admission must cover parents, descendants and unresolved cancellation.
use buzz_relay::deploy_drain::{DrainController, DrainPhase, WorkKind};
use uuid::Uuid;

#[tokio::test]
async fn periodic_worker_finishes_admitted_batch_then_joins_without_next_tick() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;
    let drain = DrainController::new();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let finished = Arc::new(AtomicUsize::new(0));
    let worker = buzz_relay::deploy_drain::periodic_worker(
        Some(drain.clone()),
        "test-reaper",
        Duration::from_secs(86400),
        {
            let entered = entered.clone();
            let release = release.clone();
            let finished = finished.clone();
            move || {
                let entered = entered.clone();
                let release = release.clone();
                let finished = finished.clone();
                async move {
                    entered.notify_one();
                    release.notified().await;
                    finished.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(())
                }
            }
        },
    );
    let worker = tokio::spawn(worker);
    entered.notified().await;
    let op = Uuid::new_v4();
    let boot = drain.begin(op).unwrap().boot_id;
    drain.fence_http(op, boot, "test-route-confirmed").unwrap();
    assert_eq!(drain.snapshot().background_inflight, 1);
    assert!(!drain.snapshot().safe_to_stop);
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(1), worker)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    assert!(drain.snapshot().safe_to_stop);
}

#[tokio::test]
async fn periodic_worker_registered_before_begin_never_runs_a_late_claim() {
    use std::time::Duration;
    let drain = DrainController::new();
    let worker = buzz_relay::deploy_drain::periodic_worker(
        Some(drain.clone()),
        "test-reaper",
        Duration::from_secs(86400),
        || async {
            panic!("fenced worker must not claim");
            #[allow(unreachable_code)]
            Ok::<_, ()>(())
        },
    );
    let op = Uuid::new_v4();
    let boot = drain.begin(op).unwrap().boot_id;
    drain.fence_http(op, boot, "test-route-confirmed").unwrap();
    assert!(!drain.snapshot().safe_to_stop);
    worker.await;
    assert!(drain.snapshot().safe_to_stop);
}

#[test]
fn begin_preserves_http_until_a_matching_routing_fence() {
    let controller = DrainController::new();
    let operation = Uuid::new_v4();
    let before = controller.snapshot();
    assert_eq!(before.phase, DrainPhase::Serving);
    assert!(!before.safe_to_stop);
    let begun = controller.begin(operation).expect("begin drain");
    assert_eq!(begun.phase, DrainPhase::Quiescing);
    assert!(begun.root_http_open);
    assert!(begun.autonomous_admissions_stopped);
    assert!(
        !begun.safe_to_stop,
        "zero counters cannot replace HTTP fencing"
    );
    assert_eq!(
        controller.begin(operation).expect("idempotent begin"),
        begun
    );
    assert!(controller.begin(Uuid::new_v4()).is_err());
    assert!(controller.try_admit_root(WorkKind::Background).is_err());
    let pending_http = controller
        .try_admit_root(WorkKind::Http)
        .expect("old edge buffered request");
    assert!(controller
        .fence_http(operation, Uuid::new_v4(), "evidence:retired")
        .is_err());
    assert!(controller
        .fence_http(Uuid::new_v4(), before.boot_id, "evidence:retired")
        .is_err());
    assert!(controller
        .fence_http(operation, before.boot_id, " ")
        .is_err());
    assert!(controller.snapshot().root_http_open);
    let fenced = controller
        .fence_http(operation, before.boot_id, "evidence:retired")
        .expect("qualified fence");
    assert_eq!(fenced.phase, DrainPhase::DrainingWork);
    assert!(!fenced.root_http_open);
    assert!(!fenced.safe_to_stop);
    assert!(controller.try_admit_root(WorkKind::Http).is_err());
    pending_http.complete();
    let finished = controller.snapshot();
    assert_eq!(finished.phase, DrainPhase::SafeToStop);
    assert!(finished.safe_to_stop);
    assert_eq!(finished.boot_id, before.boot_id);
    assert_eq!(
        controller
            .fence_http(operation, before.boot_id, "evidence:retired")
            .expect("same fence"),
        finished
    );
    assert!(controller
        .fence_http(operation, before.boot_id, "evidence:different")
        .is_err());
}

#[test]
fn live_parent_accepts_nested_children_after_root_http_is_fenced() {
    let controller = DrainController::new();
    let parent = controller
        .try_admit_root(WorkKind::Http)
        .expect("accepted push request");
    let push = controller
        .admit_child(&parent, WorkKind::Git)
        .expect("receive-pack");
    let operation = Uuid::new_v4();
    let begun = controller.begin(operation).expect("begin");
    controller
        .fence_http(operation, begun.boot_id, "evidence:retired")
        .expect("fence");
    let callback = controller
        .admit_child(&push, WorkKind::Callback)
        .expect("first hook after fence");
    parent.complete();
    let snapshot = controller.snapshot();
    assert_eq!(snapshot.http_inflight, 0);
    assert_eq!(snapshot.git_inflight, 1);
    assert_eq!(snapshot.callback_inflight, 1);
    assert!(!snapshot.safe_to_stop);
    push.complete();
    assert!(!controller.snapshot().safe_to_stop, "child outlives parent");
    callback.complete();
    assert!(controller.snapshot().safe_to_stop);
}

#[test]
fn foreign_parent_and_root_callback_cannot_bypass_admission() {
    let controller = DrainController::new();
    let other = DrainController::new();
    let foreign = other
        .try_admit_root(WorkKind::Git)
        .expect("other process work");
    assert!(controller
        .admit_child(&foreign, WorkKind::Callback)
        .is_err());
    assert!(controller.try_admit_root(WorkKind::Callback).is_err());
    foreign.complete();
}

#[test]
fn dropped_work_remains_a_blocker_after_all_counters_reach_zero() {
    let controller = DrainController::new();
    let work = controller
        .try_admit_root(WorkKind::Background)
        .expect("worker");
    drop(work);
    let operation = Uuid::new_v4();
    let begun = controller.begin(operation).expect("begin");
    controller
        .fence_http(operation, begun.boot_id, "evidence:retired")
        .expect("fence");
    let snapshot = controller.snapshot();
    assert_eq!(snapshot.background_inflight, 0);
    assert!(!snapshot.blocking_workers.is_empty());
    assert!(
        !snapshot.safe_to_stop,
        "drop cannot acknowledge unresolved effects"
    );
}

#[tokio::test]
async fn cancelling_a_detached_task_cannot_publish_false_safe() {
    let controller = DrainController::new();
    let work = controller
        .try_admit_root(WorkKind::Background)
        .expect("accepted job");
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _work = work;
        entered_tx.send(()).expect("signal running job");
        std::future::pending::<()>().await;
    });
    entered_rx.await.expect("running job");
    task.abort();
    assert!(task.await.expect_err("aborted job").is_cancelled());
    let operation = Uuid::new_v4();
    let begun = controller.begin(operation).expect("begin");
    controller
        .fence_http(operation, begun.boot_id, "evidence:retired")
        .expect("fence");
    assert!(!controller.snapshot().safe_to_stop);
}

#[test]
fn every_registered_worker_must_acknowledge_quiescence_after_admission_closes() {
    let controller = DrainController::new();
    controller
        .register_worker("audit-queue")
        .expect("register before worker starts");
    assert!(
        controller.worker_quiesced("audit-queue").is_err(),
        "serving worker cannot pre-ack drain"
    );
    let operation = Uuid::new_v4();
    let begun = controller.begin(operation).expect("begin");
    controller
        .fence_http(operation, begun.boot_id, "evidence:retired")
        .expect("fence");
    let pending = controller.snapshot();
    assert!(!pending.safe_to_stop);
    assert_eq!(pending.blocking_workers, vec!["audit-queue"]);
    assert!(controller.worker_quiesced("unknown-worker").is_err());
    controller
        .worker_quiesced("audit-queue")
        .expect("known worker joined or fenced");
    assert!(controller.snapshot().safe_to_stop);
    assert!(controller.register_worker("late-worker").is_err());
    assert!(
        !controller.snapshot().safe_to_stop,
        "late runtime mutation invalidates completion"
    );
}

#[tokio::test]
async fn asynchronous_parent_scope_carries_lineage_through_fence_without_becoming_a_root() {
    let controller = DrainController::new();
    let other = DrainController::new();
    assert!(controller
        .admit_current_child(WorkKind::Background)
        .is_err());
    let parent = controller
        .try_admit_root(WorkKind::Http)
        .expect("accepted request");
    let child = parent
        .scope(async {
            let operation = Uuid::new_v4();
            let begun = controller.begin(operation).expect("begin");
            controller
                .fence_http(operation, begun.boot_id, "evidence:retired")
                .expect("fence");
            tokio::task::yield_now().await;
            assert!(
                other.admit_current_child(WorkKind::Background).is_err(),
                "context cannot cross relay boots"
            );
            controller
                .admit_current_child(WorkKind::Background)
                .expect("accepted descendant after root fence")
        })
        .await;
    assert!(
        controller
            .admit_current_child(WorkKind::Background)
            .is_err(),
        "context cannot leak out of its future"
    );
    parent.complete();
    assert_eq!(controller.snapshot().http_inflight, 0);
    assert_eq!(controller.snapshot().background_inflight, 1);
    assert!(!controller.snapshot().safe_to_stop);
    child.complete();
    assert!(controller.snapshot().safe_to_stop);
}

#[tokio::test]
async fn suspended_parent_context_cannot_authorize_an_unrelated_sibling_task() {
    let controller = DrainController::new();
    let parent = controller
        .try_admit_root(WorkKind::Http)
        .expect("accepted request");
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        parent
            .scope(async {
                entered_tx.send(()).expect("scope entered");
                finish_rx.await.expect("finish parent scope");
            })
            .await;
        parent.complete();
    });
    entered_rx.await.expect("parent scope suspended");
    assert!(
        controller
            .admit_current_child(WorkKind::Background)
            .is_err(),
        "a sibling task must not inherit another future's accepted lineage"
    );
    finish_tx.send(()).expect("finish request");
    task.await.expect("request complete");
    assert_eq!(controller.snapshot().http_inflight, 0);
}
