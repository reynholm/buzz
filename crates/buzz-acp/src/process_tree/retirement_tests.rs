use super::retirement::*;
use std::collections::HashSet;

#[test]
fn retirement_snapshot_parses_groups_and_excludes_harness_and_reserved_groups() {
    let snapshot = RetirementSnapshot::parse("10 1 10 S harness\n20 10 20 S adapter\n21 20 21 S shell\n22 21 21 S sleep 60\n23 20 10 S unsafe-own\n24 20 1 S init-group\n25 20 0 S zero\n").unwrap();
    assert_eq!(snapshot.descendant_groups(20, 10), HashSet::from([20, 21]));
}

#[test]
fn retirement_groups_require_matching_process_identity_and_protect_other_workers() {
    let first =
        RetirementSnapshot::parse("20 10 20 S adapter\n21 20 21 S shell\n22 21 21 S sleep 60\n")
            .unwrap();
    let last = RetirementSnapshot::parse(
        "21 1 21 S shell\n22 21 21 S sleep 60\n30 10 30 S other-adapter\n31 30 31 S other-work\n",
    )
    .unwrap();
    assert_eq!(first.verified_groups(&last, 20, 10), HashSet::from([21]));
    let reused = RetirementSnapshot::parse("21 1 21 S unrelated\n").unwrap();
    assert!(first.verified_groups(&reused, 20, 10).is_empty());
}

#[test]
fn retirement_initial_snapshot_requires_live_worker_in_its_own_group() {
    let live = RetirementSnapshot::parse("20 10 20 S adapter\n").unwrap();
    assert!(live.worker_present(20));
    assert!(!live.worker_present(21));
    let zombie = RetirementSnapshot::parse("20 10 20 Z adapter\n").unwrap();
    assert!(!zombie.worker_present(20));
    let wrong = RetirementSnapshot::parse("20 10 99 S adapter\n").unwrap();
    assert!(!wrong.worker_present(20));
}

#[test]
fn native_retirement_preserves_owned_orphan_anchors_and_excludes_pid_reuse() {
    let first=RetirementSnapshot::parse_instances("20 10 20 S Wed Oct 28 20:00:00 2026 adapter\n21 20 21 S Wed Oct 28 20:00:01 2026 child\n22 21 22 S Wed Oct 28 20:00:02 2026 grandchild\n").unwrap();
    let orphan=RetirementSnapshot::parse_instances("20 10 20 S Wed Oct 28 20:00:00 2026 adapter\n21 1 21 S Wed Oct 28 20:00:01 2026 child\n22 21 22 S Wed Oct 28 20:00:02 2026 grandchild\n30 10 30 S Wed Oct 28 20:00:03 2026 other-worker\n").unwrap();
    let retained = orphan.retain_owned(Some(&first), 20).unwrap();
    let last=RetirementSnapshot::parse_instances("21 10 21 S Wed Oct 28 20:00:01 2026 child\n22 21 22 S Wed Oct 28 20:00:02 2026 grandchild\n30 10 30 S Wed Oct 28 20:00:03 2026 other-worker\n").unwrap();
    assert_eq!(
        retained.verified_groups(&last, 20, 10),
        HashSet::from([21, 22])
    );
    let reused =
        RetirementSnapshot::parse_instances("21 1 21 S Wed Oct 28 21:00:01 2026 child\n").unwrap();
    assert!(
        retained.verified_groups(&reused, 20, 10).is_empty(),
        "the same command with a different birth is a different process"
    );
}

#[test]
fn native_retirement_retention_is_bounded() {
    let mut raw = "20 10 20 S Wed Oct 28 20:00:00 2026 adapter\n".to_string();
    for pid in 21..4117 {
        raw.push_str(&format!(
            "{pid} 20 {pid} S Wed Oct 28 20:00:01 2026 child\n"
        ));
    }
    let tree = RetirementSnapshot::parse_instances(&raw).unwrap();
    assert!(tree.retain_owned(None, 20).is_none());
}
