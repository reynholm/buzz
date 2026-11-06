use super::*;

#[test]
fn sleep_inhibitor_commands_exclude_ordinary_work() {
    for (command, helper) in [
        ("caffeinate -is", true),
        ("/usr/bin/caffeinate -is", true),
        ("systemd-inhibit --what=sleep --who=Qwen Code --why=Working on a request --mode=block sleep infinity", true),
        ("/usr/bin/systemd-inhibit --no-ask-password --what=sleep --who=Qwen Code --why=Working --mode=block sleep infinity", true),
        ("caffeinate", false),
        ("caffeinate -is make test", false),
        ("caffeinate -is -t 60", false),
        ("caffeinate -w 123", false),
        ("my-caffeinate -is", false),
        ("/bin/sh -c caffeinate -is", false),
        ("python3 task.py --label caffeinate -is", false),
        ("systemd-inhibit --what=sleep --who=Qwen Code --why=Working --mode=block sleep 60", false),
        ("systemd-inhibit --what=sleep --who=Qwen Code --why=Working --mode=block make test", false),
        ("systemd-inhibit --what=sleep --who=Other --why=Working --mode=block sleep infinity", false),
        ("systemd-inhibit --help", false),
        ("sleep infinity", false),
    ] {
        assert_eq!(is_resident_helper(command), helper, "{command}");
    }
}

#[test]
fn linux_inhibitor_sleep_is_only_resident_under_the_matching_parent() {
    for (child, expected) in [
        ("/usr/bin/sleep infinity", HashSet::from([30])),
        ("sleep 60", HashSet::from([21, 30])),
        ("python3 task.py", HashSet::from([21, 30])),
    ] {
        let tree = parse_snapshot(&format!(
            "20 10 S /usr/bin/systemd-inhibit --what=sleep --who=Qwen Code --why=Working --mode=block sleep infinity\n\
             21 20 S {child}\n30 10 S sleep infinity\n"
        )).unwrap();
        let blocking: HashSet<_> = [10, 20]
            .into_iter()
            .flat_map(|parent| tree.blocking_pids(parent, &HashSet::new()))
            .collect();
        assert_eq!(blocking, expected, "{child}");
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn native_caffeinate_is_resident_while_ordinary_sleep_blocks() {
    use std::time::{Duration, Instant};

    let mut inhibitor = tokio::process::Command::new("/usr/bin/caffeinate")
        .arg("-is")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut work = tokio::process::Command::new("/bin/sleep")
        .arg("60")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let inhibitor_pid = inhibitor.id().unwrap();
    let work_pid = work.id().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut observed = None;
    while Instant::now() < deadline {
        if let Some(tree) = snapshot() {
            let children = tree.children(std::process::id());
            if children.iter().any(|child| {
                child.pid == inhibitor_pid && child.command.ends_with("caffeinate -is")
            }) && children
                .iter()
                .any(|child| child.pid == work_pid && child.command.ends_with("sleep 60"))
            {
                observed = Some(tree.blocking_pids(std::process::id(), &HashSet::new()));
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    inhibitor.kill().await.unwrap();
    work.kill().await.unwrap();
    let blocking = observed.expect("native processes reached their executable identities");
    assert!(!blocking.contains(&inhibitor_pid));
    assert!(blocking.contains(&work_pid));
}
