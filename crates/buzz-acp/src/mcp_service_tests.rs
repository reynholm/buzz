use super::*;
use std::collections::HashSet;

#[test]
fn registration_matches_worker_and_process_instance_without_hiding_work() {
    let registry = ServiceRegistry::new(std::path::PathBuf::from("/bin/wrapper")).unwrap();
    let original = "10 1 S Mon Oct 26 10:00:00 2026 adapter\n11 10 S Mon Oct 26 10:00:01 2026 wrapper\n12 11 S Mon Oct 26 10:00:01 2026 npm start\n13 12 S Mon Oct 26 10:00:01 2026 node ./src/index.mjs\n";
    let tree = crate::process_tree::parse_instance_snapshot(original).unwrap();
    registry.register(Uuid::new_v4(), 11, &tree).unwrap();
    let work = format!("{original}14 13 S Mon Oct 26 10:00:02 2026 python3 work.py\n");
    let mut tree = crate::process_tree::parse_instance_snapshot(&work).unwrap();
    registry.apply(&mut tree);
    for (parent, expected) in [
        (10, HashSet::new()),
        (11, HashSet::new()),
        (12, HashSet::new()),
        (13, HashSet::from([14])),
    ] {
        assert_eq!(tree.blocking_pids(parent, &HashSet::new()), expected);
    }
    // The same PID and command with a different birth stamp are a new process.
    let reused = original.replace("13 12 S Mon Oct 26 10:00:01", "13 12 S Mon Oct 26 10:01:01");
    let mut tree = crate::process_tree::parse_instance_snapshot(&reused).unwrap();
    registry.apply(&mut tree);
    assert_eq!(tree.blocking_pids(12, &HashSet::new()), HashSet::from([13]));
    let other = ServiceRegistry::new(std::path::PathBuf::from("/bin/wrapper")).unwrap();
    let mut tree = crate::process_tree::parse_instance_snapshot(original).unwrap();
    other.apply(&mut tree);
    assert_eq!(tree.blocking_pids(10, &HashSet::new()), HashSet::from([11]));
}

#[test]
fn stdio_wrapping_keeps_arguments_and_environment_literal() {
    let registry = ServiceRegistry::new(std::path::PathBuf::from("/bin/wrapper")).unwrap();
    let original = crate::acp::McpServer {
        name: "service".into(),
        command: "npm".into(),
        args: vec!["--prefix".into(), "/tmp/a b'c".into(), "start".into()],
        env: vec![crate::acp::EnvVar {
            name: "TEST".into(),
            value: "literal $value".into(),
        }],
    };
    let wrapped = registry.wrap_servers(vec![original]);
    assert_eq!(wrapped[0].command, "/bin/wrapper");
    assert_eq!(
        &wrapped[0].args[4..],
        ["npm", "--prefix", "/tmp/a b'c", "start"]
    );
    assert_eq!(wrapped[0].env[0].value, "literal $value");
}
