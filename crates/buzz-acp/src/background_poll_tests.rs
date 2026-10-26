use super::background_poll::BackgroundPoll;
use crate::process_tree::ProcessTree;
use std::sync::{Arc, Barrier};
use std::time::Duration;

#[tokio::test(flavor = "current_thread")]
async fn slow_snapshot_keeps_event_loop_responsive_and_allows_only_one_poll() {
    let gate = Arc::new(Barrier::new(2));
    let worker_gate = gate.clone();
    let mut poll = BackgroundPoll::default();
    assert!(poll.start_with(vec![], move || {
        worker_gate.wait();
        Some(ProcessTree::default())
    }));
    assert!(!poll.start_with(vec![], || panic!("second poll must not start")));
    // A main-loop input must win while enumeration is still running.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tx.send("restart").unwrap();
    let winner = tokio::time::timeout(Duration::from_millis(200), async {
        tokio::select! {
            biased;
            _ = poll.wait() => panic!("snapshot must still be blocked"),
            value = rx.recv() => value,
        }
    })
    .await;
    // Always release the blocking thread, even when the assertion fails.
    gate.wait();
    assert_eq!(winner.unwrap(), Some("restart"));
    assert!(poll.wait().await.unwrap().unwrap().tree.is_some());
    assert!(!poll.in_flight());
    assert!(poll.start_with(vec![], || None));
    assert!(poll.wait().await.unwrap().unwrap().tree.is_none());
}
