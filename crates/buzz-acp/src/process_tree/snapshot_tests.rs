use super::*;
use std::time::{Duration, Instant};

#[test]
fn snapshot_rejects_partial_output_and_ignores_zombies() {
    let tree = parse_snapshot(
        "10 1 S worker\n20 10 Z gone\n21 10 S python3 provider.py\n30 21 S sleep 60\n",
    )
    .expect("valid snapshot");
    assert_eq!(tree.descendant_pids(10), HashSet::from([21, 30]));
    assert_eq!(tree.children(10).len(), 1);
    assert!(parse_snapshot("10 1 S worker\ntruncated entry\n").is_none());
}

#[tokio::test]
async fn listing_timeout_kills_a_stalled_command() {
    let marker = tempfile::NamedTempFile::new().expect("pid marker");
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .args([
            "-c",
            "printf '%s' \"$$\" > \"$1\"; exec /bin/sleep 60",
            "listing-probe",
        ])
        .arg(marker.path());
    let start = Instant::now();
    assert!(bounded_output(command, Duration::from_millis(100), 1024).is_none());
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "enumeration must not wait for a stalled process"
    );
    let pid: i32 = std::fs::read_to_string(marker.path())
        .expect("probe started")
        .trim()
        .parse()
        .expect("probe pid");
    let pid = nix::unistd::Pid::from_raw(pid);
    let mut exited = false;
    for _ in 0..50 {
        if nix::sys::signal::kill(pid, None) == Err(nix::errno::Errno::ESRCH) {
            exited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !exited {
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
    }
    assert!(
        exited,
        "timed-out enumeration must kill and reap the subprocess"
    );
}

#[tokio::test]
async fn listing_rejects_excess_output_and_nonzero_exit() {
    let mut excess = tokio::process::Command::new("/bin/sh");
    excess.args(["-c", "while :; do printf 1234567890; done"]);
    let start = Instant::now();
    assert!(bounded_output(excess, Duration::from_secs(1), 128).is_none());
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "the output limit must stop capture before the deadline"
    );
    let mut failed = tokio::process::Command::new("/bin/sh");
    failed.args(["-c", "printf partial; exit 1"]);
    assert!(bounded_output(failed, Duration::from_secs(1), 128).is_none());
    let mut success = tokio::process::Command::new("/bin/sh");
    success.args(["-c", "printf complete"]);
    assert_eq!(
        bounded_output(success, Duration::from_secs(1), 128).as_deref(),
        Some("complete")
    );
}
