use super::*;
use std::os::unix::process::ExitStatusExt;
use std::time::{Duration, Instant};

async fn probe(setup: &str, body: &str) -> AcpClient {
    let script = format!(
        "import os\n{setup}\nprint('BUZZ_PROBE_READY:' + str(os.getpid()), flush=True)\n{body}"
    );
    let mut client = AcpClient::spawn(
        "/usr/bin/python3",
        &["-u".into(), "-c".into(), script],
        &[],
        false,
    )
    .await
    .unwrap();
    let expected = format!("BUZZ_PROBE_READY:{}", client.process_id().unwrap());
    let ready = tokio::time::timeout(Duration::from_secs(10), client.reader.next()).await;
    if !matches!(&ready, Ok(Some(Ok(line))) if line == &expected) {
        let report = client.terminate().await;
        client.shutdown().await;
        panic!("probe setup did not reach its readiness barrier: {ready:?}; cleanup={report:?}");
    }
    assert!(
        client.child.try_wait().unwrap().is_none(),
        "probe exited after readiness"
    );
    client
}

#[tokio::test]
async fn checked_termination_waits_for_the_group_after_leader_exits() {
    let mut client = probe(
        "import subprocess,sys; child=subprocess.Popen(['sleep','2']); assert child.poll() is None",
        "sys.stdin.read()",
    )
    .await;
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(client.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn checked_termination_escalates_to_term_at_three_seconds() {
    let mut client = probe(
        "import signal,time; assert signal.getsignal(signal.SIGTERM) == signal.SIG_DFL",
        "time.sleep(60)",
    )
    .await;
    let started = Instant::now();
    let report = client.terminate().await;
    let elapsed = started.elapsed();
    let status = client.child.try_wait().unwrap();
    assert_eq!(
        report,
        TerminationReport::Clean,
        "elapsed={elapsed:?}, exit={status:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(3),
        "elapsed={elapsed:?}, exit={status:?}"
    );
    assert!(
        elapsed < Duration::from_secs(6),
        "elapsed={elapsed:?}, exit={status:?}"
    );
    assert_eq!(
        status.and_then(|exit| exit.signal()),
        Some(nix::sys::signal::Signal::SIGTERM as i32),
        "elapsed={elapsed:?}, exit={status:?}"
    );
}

#[tokio::test]
async fn checked_termination_escalates_to_kill_at_fifteen_seconds() {
    let mut client =
        probe("import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); assert signal.getsignal(signal.SIGTERM) == signal.SIG_IGN", "time.sleep(60)")
            .await;
    let started = Instant::now();
    let report = client.terminate().await;
    let elapsed = started.elapsed();
    let status = client.child.try_wait().unwrap();
    assert_eq!(
        report,
        TerminationReport::Clean,
        "elapsed={elapsed:?}, exit={status:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(15),
        "elapsed={elapsed:?}, exit={status:?}"
    );
    assert!(
        elapsed < Duration::from_secs(18),
        "elapsed={elapsed:?}, exit={status:?}"
    );
    assert_eq!(
        status.and_then(|exit| exit.signal()),
        Some(nix::sys::signal::Signal::SIGKILL as i32),
        "elapsed={elapsed:?}, exit={status:?}"
    );
}

#[tokio::test]
async fn checked_termination_three_bounded_snapshot_failures_preserve_worker() {
    let mut client = probe("import sys", "sys.stdin.read()").await;
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
    let setup = format!("import subprocess,sys\nchild=subprocess.Popen(['sleep','60'],start_new_session=True)\nwith open({:?},'w') as marker: marker.write(str(child.pid))\nassert child.poll() is None", marker.path().to_string_lossy());
    let mut client = probe(&setup, "sys.stdin.read()").await;
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
    let mut client = probe("import sys", "sys.stdin.read()").await;
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
    let mut client = probe("import sys", "sys.stdin.read()").await;
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(client.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn checked_termination_retains_disappeared_groups_across_explicit_retry() {
    let mut client = probe("import sys", "sys.stdin.read()").await;
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    let started = Instant::now();
    assert_eq!(client.terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() < Duration::from_secs(2));
}
