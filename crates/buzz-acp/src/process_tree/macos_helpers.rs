//! Codex starts its code-mode service lazily, after session setup. Its lifetime
//! is not a background task. macOS `ps comm` preserves its executable path,
//! including spaces, without confusing arguments of other work for helpers.
use super::*;

const CODE_MODE_HOST: &str = "codex-code-mode-host";

pub(super) fn classify(mut tree: ProcessTree) -> ProcessTree {
    let candidates: HashSet<u32> = tree
        .children
        .values()
        .flatten()
        .filter(|child| {
            child
                .command
                .split_whitespace()
                .any(|word| word.rsplit('/').next() == Some(CODE_MODE_HOST))
        })
        .map(|child| child.pid)
        .collect();
    // Bound argv as well as time/output. Uncertain enumeration keeps work held.
    if candidates.is_empty() || candidates.len() > 128 {
        return tree;
    }
    let pids = candidates
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut command = tokio::process::Command::new("/bin/ps");
    command.args(["-p", &pids, "-o", "pid=,comm="]);
    let output = bounded_output(command, std::time::Duration::from_millis(250), 256 * 1024);
    tree.resident_helpers = confirmed_hosts(output.as_deref(), &candidates);
    tree
}

fn confirmed_hosts(output: Option<&str>, candidates: &HashSet<u32>) -> HashSet<u32> {
    let Some(output) = output else {
        return HashSet::new();
    };
    output
        .lines()
        .filter_map(|line| {
            let (pid, executable) = line.trim().split_once(char::is_whitespace)?;
            let pid = pid.parse().ok()?;
            // Do not collapse whitespace inside an executable path. No filesystem
            // lookup here: a mounted filesystem must not stall the pool poll.
            let name = std::path::Path::new(executable.trim())
                .file_name()?
                .to_str()?;
            (candidates.contains(&pid) && name == CODE_MODE_HOST).then_some(pid)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_identity_handles_spaces_and_rejects_work_arguments() {
        let candidates = HashSet::from([10, 11, 12, 13]);
        assert_eq!(
            confirmed_hosts(
                Some(
                    "10 /Applications/Application  Support/bin/codex-code-mode-host\n\
             11 /bin/cat\n12 /bin/bash\n13 /tmp/codex-code-mode-host-worker\n\
             14 /tmp/codex-code-mode-host\n"
                ),
                &candidates
            ),
            HashSet::from([10])
        );
    }

    #[test]
    fn failed_or_missing_executable_confirmation_keeps_work_blocking() {
        let candidates = HashSet::from([10]);
        for output in [None, Some(""), Some("invalid"), Some("10 codex-code-mode")] {
            assert!(confirmed_hosts(output, &candidates).is_empty());
        }
    }
}
