//! Recognize only the default Bash capture emitted by Codex shell snapshots.
use super::*;

// Codex d27764b82f7118f674371e6d6e76271d9d606edb, shell-command/src/
// startup.rs + shell_snapshot_capture.rs + shell_snapshot_exports.rs:
// interactive startup, Eval replay, declarations enabled, environment disabled.
const BASH_CAPTURE: &str = include_str!("codex_bash_snapshot.txt");

pub(super) fn candidates(tree: &ProcessTree) -> HashMap<u32, u32> {
    let mut candidates = HashMap::new();
    let servers: HashSet<_> = tree
        .children
        .values()
        .flatten()
        .filter(|process| is_app_server(&process.command))
        .map(|process| process.pid)
        .collect();
    for (parent, children) in &tree.children {
        if !servers.contains(parent) {
            continue;
        }
        for child in children {
            if is_bash_capture(&child.command) {
                candidates.insert(child.pid, *parent);
            }
        }
    }
    candidates
}

fn is_bash_capture(command: &str) -> bool {
    let mut args = command.split_whitespace();
    args.next().is_some_and(|path| basename(path) == "bash")
        && args.next() == Some("-lc")
        && args.eq(BASH_CAPTURE.split_whitespace())
}

fn is_app_server(command: &str) -> bool {
    let mut args = command.split_whitespace();
    if args.next().is_none_or(|path| basename(path) != "codex") {
        return false;
    }
    loop {
        match args.next() {
            Some("-c" | "--config") if args.next().is_some() => {}
            Some("app-server") => return args.next().is_none(),
            _ => return false,
        }
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(super) fn classify_identities(
    candidates: &HashMap<u32, u32>,
    output: Option<&str>,
) -> (HashSet<u32>, HashSet<u32>) {
    let identities: HashMap<_, _> = output
        .into_iter()
        .flat_map(str::lines)
        .filter_map(|line| {
            let (pid, executable) = line.trim().split_once(char::is_whitespace)?;
            let executable = executable.trim();
            #[cfg(target_os = "linux")]
            let executable = executable.strip_suffix(" (deleted)").unwrap_or(executable);
            let name = basename(executable);
            if name.is_empty() || name == "-" || name == "?" {
                return None;
            }
            Some((pid.parse::<u32>().ok()?, name))
        })
        .collect();
    let mut confirmed = HashSet::new();
    let mut uncertain = HashSet::new();
    for (child, parent) in candidates {
        match (
            identities.get(child).copied(),
            identities.get(parent).copied(),
        ) {
            (Some("bash"), Some("codex")) => {
                confirmed.insert(*child);
            }
            (Some(name), _) if name != "bash" => {}
            (_, Some(name)) if name != "codex" => {}
            _ => {
                uncertain.insert(*child);
            }
        }
    }
    (confirmed, uncertain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(shell: &str, parent: &str) -> ProcessTree {
        parse_snapshot(&format!("20 10 S {parent}\n30 20 S {shell}\n")).unwrap()
    }

    #[test]
    fn snapshot_requires_complete_script_exact_flags_and_direct_app_server() {
        let shell = format!("/bin/bash -lc {BASH_CAPTURE}");
        // ps collapses whitespace; normalize fixture rows exactly as production does.
        let shell = shell.split_whitespace().collect::<Vec<_>>().join(" ");
        for parent in [
            "/opt/codex app-server",
            "/opt/codex -c mcp_servers.test.enabled=false app-server",
        ] {
            assert_eq!(candidates(&tree(&shell, parent)), HashMap::from([(30, 20)]));
        }
        for command in [
            format!("{shell} ; sleep 60"),
            format!("/bin/bash -lc sleep 60 # {BASH_CAPTURE}"),
            shell.replacen("-lc", "-c", 1),
            shell.replacen("declare -f", "sleep 60", 1),
            format!("/bin/bash -c read marker {shell}"),
        ] {
            let command = command.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(candidates(&tree(&command, "/opt/codex app-server")).is_empty());
        }
        for parent in [
            "/bin/bash app-server",
            "/opt/codex exec app-server",
            "/opt/codex -c app-server",
        ] {
            assert!(candidates(&tree(&shell, parent)).is_empty());
        }
    }

    #[test]
    fn snapshot_requires_both_kernel_identities_and_known_probe() {
        let candidates = HashMap::from([(30, 20)]);
        assert_eq!(
            classify_identities(&candidates, Some("20 /opt/codex\n30 /bin/bash\n")).0,
            HashSet::from([30])
        );
        for output in [
            None,
            Some(""),
            Some("20 /bin/python3\n30 /bin/bash\n"),
            Some("20 /opt/codex\n30 /bin/python3\n"),
            Some("20 /opt/codex\n30 -\n"),
            Some("30 /bin/bash\n"),
        ] {
            assert!(classify_identities(&candidates, output).0.is_empty());
        }
    }
    #[test]
    fn snapshot_identity_does_not_promote_ordinary_ancestors() {
        let shell = format!("/bin/bash -lc {BASH_CAPTURE}")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut tree = parse_snapshot(&format!(
            "15 10 S /bin/bash user-task.sh\n20 15 S /opt/codex app-server\n30 20 S {shell}\n"
        ))
        .unwrap();
        tree.snapshot_helpers.insert(30);
        assert!(tree.is_resident_helper(&tree.children(20)[0]));
        assert!(!tree.is_resident_helper(&tree.children(15)[0]));
        assert!(!tree.is_resident_helper(&tree.children(10)[0]));
        assert_eq!(tree.blocking_pids(10, &HashSet::new()), HashSet::from([15]));
    }
    #[test]
    fn missing_identity_is_uncertain_but_positive_mismatch_is_work() {
        let candidates = HashMap::from([(30, 20)]);
        for output in [
            None,
            Some("20 /opt/codex\n"),
            Some("30 /bin/bash\n"),
            Some("20 /opt/codex\n30 -\n"),
        ] {
            assert_eq!(
                classify_identities(&candidates, output),
                (HashSet::new(), HashSet::from([30]))
            );
        }
        for output in [Some("20 /usr/bin/python3\n"), Some("30 /usr/bin/python3\n")] {
            assert_eq!(
                classify_identities(&candidates, output),
                (HashSet::new(), HashSet::new())
            );
        }
    }
}
