use super::*;
use std::time::{Duration, Instant};

async fn probe(script: &str) -> AcpClient {
    let client = AcpClient::spawn(
        "/usr/bin/python3",
        &["-u".into(), "-c".into(), script.into()],
        &[],
        false,
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    client
}

#[tokio::test]
async fn checked_termination_waits_for_the_group_after_leader_exits() {
    let mut client =
        probe("import subprocess,sys; subprocess.Popen(['sleep','2']); sys.stdin.read()").await;
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(client.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn checked_termination_escalates_to_term_at_three_seconds() {
    let mut client = probe("import time; time.sleep(60)").await;
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() >= Duration::from_secs(3));
    assert!(started.elapsed() < Duration::from_secs(6));
    assert!(client.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn checked_termination_escalates_to_kill_at_fifteen_seconds() {
    let mut client =
        probe("import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(60)")
            .await;
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() >= Duration::from_secs(15));
    assert!(started.elapsed() < Duration::from_secs(18));
    assert!(client.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn checked_termination_three_bounded_snapshot_failures_preserve_worker() {
    let mut client = probe("import sys; sys.stdin.read()").await;
    let started = Instant::now();
    let mut attempts = 0;
    let report = client
        .terminate_with_snapshot(|| {
            attempts += 1;
            async {
                tokio::task::spawn_blocking(|| {
                    let mut command = tokio::process::Command::new("/bin/sleep");
                    command.arg("60");
                    assert!(crate::process_tree::bounded_output(
                        command,
                        Duration::from_secs(2),
                        1024
                    )
                    .is_none());
                    None
                })
                .await
                .unwrap()
            }
        })
        .await;
    assert_eq!(report, TerminationReport::Unverified);
    assert_eq!(attempts, 3);
    assert!(started.elapsed() >= Duration::from_secs(6));
    assert!(started.elapsed() < Duration::from_secs(9));
    assert!(
        client.stdin.is_some(),
        "uncertainty must not close the provider's stdin"
    );
    assert!(
        client.child.try_wait().unwrap().is_none(),
        "no signal on failed initial snapshots"
    );
    client.shutdown().await;
}

#[tokio::test]
async fn checked_termination_kills_snapshotted_separate_shell_group() {
    let marker = tempfile::NamedTempFile::new().unwrap();
    let script = format!("import subprocess,sys; child=subprocess.Popen(['sleep','60'],start_new_session=True); open({:?},'w').write(str(child.pid)); sys.stdin.read()", marker.path().to_string_lossy());
    let mut client = probe(&script).await;
    let pid: i32 = std::fs::read_to_string(marker.path())
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert_eq!(
        nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), None),
        Err(nix::errno::Errno::ESRCH)
    );
    assert!(client.child.try_wait().unwrap().is_some());
    assert!(nix::sys::signal::killpg(nix::unistd::getpgrp(), None).is_ok());
}

#[tokio::test]
async fn checked_termination_missing_worker_in_initial_snapshot_never_starts() {
    let mut client = probe("import sys; sys.stdin.read()").await;
    let mut attempts = 0;
    let report = client
        .terminate_with_snapshot(|| {
            attempts += 1;
            std::future::ready(crate::process_tree::retirement::RetirementSnapshot::parse(
                "1 0 1 S init\n",
            ))
        })
        .await;
    assert_eq!(attempts, 3);
    assert_eq!(report, TerminationReport::Unverified);
    assert!(client.stdin.is_some());
    assert!(client.child.try_wait().unwrap().is_none());
    client.shutdown().await;
}

#[tokio::test]
async fn checked_termination_zombie_only_group_does_not_consume_kill_grace() {
    let mut client = probe("import sys; sys.stdin.read()").await;
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(client.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn checked_termination_retains_disappeared_groups_across_explicit_retry() {
    let mut client = probe("import sys; sys.stdin.read()").await;
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() < Duration::from_secs(2));
}
